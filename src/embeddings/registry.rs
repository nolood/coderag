use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use super::config::{EnhancedEmbeddingsConfig, ProviderType};
use super::provider::{EmbeddingProvider, HealthStatus, ProviderInfo};
use super::fastembed_provider::FastEmbedProvider;
use super::openai_provider::OpenAIProvider;

/// Registry for managing embedding providers
pub struct ProviderRegistry {
    providers: Arc<RwLock<HashMap<String, Arc<dyn EmbeddingProvider>>>>,
    active_provider: Arc<RwLock<String>>,
    fallback_chain: Vec<String>,
    config: EnhancedEmbeddingsConfig,
}

impl ProviderRegistry {
    /// Create a new registry with default providers from configuration
    pub async fn new(config: &EnhancedEmbeddingsConfig) -> Result<Self> {
        let providers = Arc::new(RwLock::new(HashMap::new()));
        let active_provider = Arc::new(RwLock::new(String::new()));

        let mut registry = Self {
            providers,
            active_provider,
            fallback_chain: config.fallback_chain.clone(),
            config: config.clone(),
        };

        // Initialize providers based on configuration
        registry.initialize_providers().await?;

        Ok(registry)
    }

    /// Initialize configured providers
    async fn initialize_providers(&mut self) -> Result<()> {
        // Initialize the active provider
        match self.config.provider {
            ProviderType::FastEmbed => {
                let provider_config = self.config.providers.fastembed
                    .clone()
                    .unwrap_or_else(|| {
                        // Use legacy config if available
                        super::config::FastEmbedConfig {
                            model: self.config.model.clone()
                                .unwrap_or_else(|| "nomic-embed-text-v1.5".to_string()),
                            batch_size: self.config.batch_size.unwrap_or(32),
                            cache_dir: None,
                        }
                    });

                let provider = Arc::new(FastEmbedProvider::new(&provider_config)?);
                self.register("fastembed".to_string(), provider).await?;
                *self.active_provider.write().await = "fastembed".to_string();
            }
            ProviderType::OpenAI => {
                let provider_config = self.config.providers.openai
                    .clone()
                    .ok_or_else(|| anyhow!("OpenAI configuration not provided"))?;

                let provider = Arc::new(OpenAIProvider::new(&provider_config).await?);
                self.register("openai".to_string(), provider).await?;
                *self.active_provider.write().await = "openai".to_string();
            }
        }

        // Initialize fallback providers
        for fallback_name in &self.fallback_chain.clone() {
            if self.providers.read().await.contains_key(fallback_name) {
                continue; // Already registered
            }

            match fallback_name.as_str() {
                "fastembed" => {
                    if let Some(config) = &self.config.providers.fastembed {
                        let provider = Arc::new(FastEmbedProvider::new(config)?);
                        self.register(fallback_name.clone(), provider).await?;
                    }
                }
                "openai" => {
                    if let Some(config) = &self.config.providers.openai {
                        let provider = Arc::new(OpenAIProvider::new(config).await?);
                        self.register(fallback_name.clone(), provider).await?;
                    }
                }
                _ => {
                    warn!("Unknown fallback provider: {}", fallback_name);
                }
            }
        }

        Ok(())
    }

    /// Register a new provider
    pub async fn register(
        &self,
        name: String,
        provider: Arc<dyn EmbeddingProvider>
    ) -> Result<()> {
        info!("Registering provider: {}", name);
        self.providers.write().await.insert(name.clone(), provider);
        Ok(())
    }

    /// Get the currently active provider
    pub async fn get_active(&self) -> Result<Arc<dyn EmbeddingProvider>> {
        let active_name = self.active_provider.read().await;
        self.get(&active_name).await
    }

    /// Get a specific provider by name
    pub async fn get(&self, name: &str) -> Result<Arc<dyn EmbeddingProvider>> {
        self.providers
            .read()
            .await
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow!("Provider not found: {}", name))
    }

    /// Switch active provider
    pub async fn switch_provider(&self, name: &str) -> Result<()> {
        // Verify provider exists
        let _ = self.get(name).await?;

        info!("Switching active provider to: {}", name);
        *self.active_provider.write().await = name.to_string();
        Ok(())
    }

    /// Get provider with automatic fallback on failure
    pub async fn get_with_fallback(&self) -> Result<Arc<dyn EmbeddingProvider>> {
        // Try active provider first
        let active_name = self.active_provider.read().await.clone();

        match self.get(&active_name).await {
            Ok(provider) => {
                match provider.health_check().await? {
                    HealthStatus::Healthy => return Ok(provider),
                    HealthStatus::Degraded { reason } => {
                        warn!("Active provider {} is degraded: {}", active_name, reason);
                        return Ok(provider); // Still use degraded provider
                    }
                    HealthStatus::Unhealthy { error } => {
                        error!("Active provider {} is unhealthy: {}", active_name, error);
                        // Fall through to try fallback providers
                    }
                }
            }
            Err(e) => {
                error!("Failed to get active provider {}: {}", active_name, e);
            }
        }

        // Try fallback chain
        for fallback_name in &self.fallback_chain {
            match self.get(fallback_name).await {
                Ok(provider) => {
                    match provider.health_check().await? {
                        HealthStatus::Healthy | HealthStatus::Degraded { .. } => {
                            info!("Using fallback provider: {}", fallback_name);
                            return Ok(provider);
                        }
                        HealthStatus::Unhealthy { error } => {
                            warn!("Fallback provider {} is unhealthy: {}", fallback_name, error);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to get fallback provider {}: {}", fallback_name, e);
                }
            }
        }

        Err(anyhow!("All providers failed"))
    }

    /// Embed with automatic fallback on failure
    pub async fn embed_with_fallback(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut errors = Vec::new();

        // Try primary provider
        let primary = self.get_active().await?;
        match primary.embed(texts).await {
            Ok(embeddings) => return Ok(embeddings),
            Err(e) => {
                warn!("Primary provider failed: {}", e);
                errors.push(format!("{}: {}", primary.provider_name(), e));
            }
        }

        // Try fallback chain
        for provider_name in &self.fallback_chain {
            match self.get(provider_name).await {
                Ok(provider) => {
                    match provider.embed(texts).await {
                        Ok(embeddings) => {
                            info!("Fallback to {} succeeded", provider_name);
                            return Ok(embeddings);
                        }
                        Err(e) => {
                            warn!("Fallback provider {} failed: {}", provider_name, e);
                            errors.push(format!("{}: {}", provider_name, e));
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("Failed to get {}: {}", provider_name, e));
                }
            }
        }

        Err(anyhow!("All providers failed: {:?}", errors))
    }

    /// List all registered providers
    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        let providers = self.providers.read().await;
        let active_name = self.active_provider.read().await;
        let mut infos = Vec::new();

        for (name, provider) in providers.iter() {
            let is_healthy = matches!(provider.health_check().await, Ok(HealthStatus::Healthy));

            infos.push(ProviderInfo {
                name: name.clone(),
                is_active: name == active_name.as_str(),
                is_healthy,
                capabilities: provider.capabilities(),
            });
        }

        infos
    }
}

/// Factory for creating providers
pub struct ProviderFactory;

impl ProviderFactory {
    /// Create a provider from type and configuration
    pub async fn create_provider(
        provider_type: ProviderType,
        config: &EnhancedEmbeddingsConfig
    ) -> Result<Arc<dyn EmbeddingProvider>> {
        match provider_type {
            ProviderType::FastEmbed => {
                let provider_config = config.providers.fastembed
                    .clone()
                    .unwrap_or_else(|| {
                        // Use legacy config if available
                        super::config::FastEmbedConfig {
                            model: config.model.clone()
                                .unwrap_or_else(|| "nomic-embed-text-v1.5".to_string()),
                            batch_size: config.batch_size.unwrap_or(32),
                            cache_dir: None,
                        }
                    });

                Ok(Arc::new(FastEmbedProvider::new(&provider_config)?))
            }
            ProviderType::OpenAI => {
                let provider_config = config.providers.openai
                    .clone()
                    .ok_or_else(|| anyhow!("OpenAI configuration not provided"))?;

                Ok(Arc::new(OpenAIProvider::new(&provider_config).await?))
            }
        }
    }

    /// Create a provider from name string
    pub async fn create_provider_by_name(
        name: &str,
        config: &EnhancedEmbeddingsConfig
    ) -> Result<Arc<dyn EmbeddingProvider>> {
        let provider_type = match name.to_lowercase().as_str() {
            "fastembed" => ProviderType::FastEmbed,
            "openai" => ProviderType::OpenAI,
            _ => return Err(anyhow!("Unknown provider type: {}", name)),
        };

        Self::create_provider(provider_type, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::{FastEmbedConfig, OpenAIConfig, ProvidersConfig};

    fn test_config() -> EnhancedEmbeddingsConfig {
        EnhancedEmbeddingsConfig {
            provider: ProviderType::FastEmbed,
            fallback_chain: vec![],
            providers: ProvidersConfig {
                fastembed: Some(FastEmbedConfig {
                    model: "all-MiniLM-L6-v2".to_string(),
                    batch_size: 32,
                    cache_dir: None,
                }),
                openai: None,
            },
            cache: Default::default(),
            retry: Default::default(),
            model: None,
            batch_size: None,
        }
    }

    #[tokio::test]
    async fn test_registry_creation() {
        let config = test_config();
        let registry = ProviderRegistry::new(&config).await;

        // Should succeed even without models downloaded
        assert!(registry.is_ok());
    }

    #[tokio::test]
    async fn test_provider_registration() {
        let config = test_config();
        let registry = ProviderRegistry::new(&config).await.unwrap();

        let providers = registry.list_providers().await;
        assert!(!providers.is_empty());

        // Should have at least the active provider
        let active = providers.iter().find(|p| p.is_active);
        assert!(active.is_some());
    }

    #[tokio::test]
    async fn test_provider_switching() {
        let mut config = test_config();

        // Add OpenAI config for testing
        config.providers.openai = Some(OpenAIConfig {
            api_key: "test-key".to_string(),
            model: "text-embedding-3-small".to_string(),
            organization: None,
            base_url: None,
            max_retries: 3,
            timeout_secs: 30,
            batch_size: 100,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            exponential_base: 2.0,
        });

        config.fallback_chain = vec!["openai".to_string()];

        let registry = ProviderRegistry::new(&config).await.unwrap();

        // Should start with fastembed
        let active = registry.get_active().await.unwrap();
        assert_eq!(active.provider_name(), "fastembed");

        // Switch to OpenAI if it was registered
        if registry.get("openai").await.is_ok() {
            registry.switch_provider("openai").await.unwrap();
            let active = registry.get_active().await.unwrap();
            assert_eq!(active.provider_name(), "openai");
        }
    }

    #[tokio::test]
    async fn test_legacy_config_support() {
        let mut config = test_config();

        // Simulate legacy configuration
        config.model = Some("nomic-embed-text-v1.5".to_string());
        config.batch_size = Some(64);
        config.providers.fastembed = None;

        let registry = ProviderRegistry::new(&config).await.unwrap();
        let active = registry.get_active().await.unwrap();

        assert_eq!(active.provider_name(), "fastembed");
        assert_eq!(active.max_batch_size(), 64);
    }
}