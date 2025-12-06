use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Provider type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    FastEmbed,
    OpenAI,
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::FastEmbed
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FastEmbed => write!(f, "fastembed"),
            Self::OpenAI => write!(f, "openai"),
        }
    }
}

/// Enhanced embeddings configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnhancedEmbeddingsConfig {
    /// Active provider name
    #[serde(default)]
    pub provider: ProviderType,

    /// Fallback providers in order of preference
    #[serde(default)]
    pub fallback_chain: Vec<String>,

    /// Provider-specific configurations
    #[serde(default)]
    pub providers: ProvidersConfig,

    /// Global settings
    #[serde(default)]
    pub cache: CacheConfig,

    #[serde(default)]
    pub retry: RetryConfig,

    /// Legacy fields for backward compatibility
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,
}

/// Provider-specific configurations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvidersConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fastembed: Option<FastEmbedConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<OpenAIConfig>,
}

/// FastEmbed provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastEmbedConfig {
    #[serde(default = "default_fastembed_model")]
    pub model: String,

    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<PathBuf>,
}

impl Default for FastEmbedConfig {
    fn default() -> Self {
        Self {
            model: default_fastembed_model(),
            batch_size: default_batch_size(),
            cache_dir: None,
        }
    }
}

fn default_fastembed_model() -> String {
    "nomic-embed-text-v1.5".to_string()
}

fn default_batch_size() -> usize {
    32
}

/// OpenAI provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key (can be environment variable reference like ${OPENAI_API_KEY})
    #[serde(default)]
    pub api_key: String,

    #[serde(default = "default_openai_model")]
    pub model: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,  // For Azure or custom endpoints

    #[serde(default = "default_max_retries")]
    pub max_retries: usize,

    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    #[serde(default = "default_openai_batch_size")]
    pub batch_size: usize,

    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,

    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,

    #[serde(default = "default_exponential_base")]
    pub exponential_base: f64,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: default_openai_model(),
            organization: None,
            base_url: None,
            max_retries: default_max_retries(),
            timeout_secs: default_timeout_secs(),
            batch_size: default_openai_batch_size(),
            initial_backoff_ms: default_initial_backoff_ms(),
            max_backoff_ms: default_max_backoff_ms(),
            exponential_base: default_exponential_base(),
        }
    }
}

fn default_openai_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_max_retries() -> usize {
    3
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_openai_batch_size() -> usize {
    100
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,

    #[serde(default = "default_ttl_seconds")]
    pub ttl_seconds: u64,

    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_cache_enabled(),
            ttl_seconds: default_ttl_seconds(),
            max_entries: default_max_entries(),
        }
    }
}

fn default_cache_enabled() -> bool {
    true
}

fn default_ttl_seconds() -> u64 {
    3600
}

fn default_max_entries() -> usize {
    10000
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_retry_max_attempts")]
    pub max_attempts: usize,

    #[serde(default = "default_initial_backoff_ms")]
    pub initial_backoff_ms: u64,

    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,

    #[serde(default = "default_exponential_base")]
    pub exponential_base: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_retry_max_attempts(),
            initial_backoff_ms: default_initial_backoff_ms(),
            max_backoff_ms: default_max_backoff_ms(),
            exponential_base: default_exponential_base(),
        }
    }
}

fn default_retry_max_attempts() -> usize {
    3
}

fn default_initial_backoff_ms() -> u64 {
    100
}

fn default_max_backoff_ms() -> u64 {
    10000
}

fn default_exponential_base() -> f64 {
    2.0
}

impl OpenAIConfig {
    /// Load API key from configuration or environment variable
    pub fn load_api_key(&self) -> anyhow::Result<String> {
        use anyhow::Context;

        // Priority order for API key sources

        // 1. Explicit configuration
        if !self.api_key.is_empty() && !self.api_key.starts_with("${") {
            return Ok(self.api_key.clone());
        }

        // 2. Environment variable reference
        if self.api_key.starts_with("${") && self.api_key.ends_with("}") {
            let var_name = &self.api_key[2..self.api_key.len()-1];
            return std::env::var(var_name)
                .with_context(|| format!("Environment variable {} not set", var_name));
        }

        // 3. Standard environment variable
        std::env::var("OPENAI_API_KEY")
            .context("No API key configured and OPENAI_API_KEY environment variable not set")
    }
}

/// Convert legacy config to enhanced config
impl EnhancedEmbeddingsConfig {
    pub fn from_legacy(model: String, batch_size: usize) -> Self {
        // Set up FastEmbed provider with legacy settings
        Self {
            provider: ProviderType::FastEmbed,
            providers: ProvidersConfig {
                fastembed: Some(FastEmbedConfig {
                    model: model.clone(),
                    batch_size,
                    cache_dir: None,
                }),
                ..Default::default()
            },
            model: Some(model),
            batch_size: Some(batch_size),
            ..Default::default()
        }
    }

    /// Get the active provider configuration
    pub fn active_provider_config(&self) -> anyhow::Result<Box<dyn std::any::Any>> {
        match self.provider {
            ProviderType::FastEmbed => {
                let config = self.providers.fastembed
                    .clone()
                    .unwrap_or_else(|| {
                        // Fall back to legacy configuration if available
                        FastEmbedConfig {
                            model: self.model.clone().unwrap_or_else(default_fastembed_model),
                            batch_size: self.batch_size.unwrap_or_else(default_batch_size),
                            cache_dir: None,
                        }
                    });
                Ok(Box::new(config))
            }
            ProviderType::OpenAI => {
                let config = self.providers.openai
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("OpenAI configuration not provided"))?;
                Ok(Box::new(config))
            }
        }
    }
}