mod provider;
mod config;
mod fastembed_provider;
mod openai_provider;
mod registry;

// Re-export public interfaces
pub use provider::{EmbeddingProvider, ProviderCapabilities, HealthStatus, ProviderInfo};
pub use config::{
    EnhancedEmbeddingsConfig, ProviderType, ProvidersConfig,
    FastEmbedConfig, OpenAIConfig, CacheConfig, RetryConfig
};
pub use registry::{ProviderRegistry, ProviderFactory};

// Re-export legacy EmbeddingGenerator for backward compatibility
pub use fastembed_provider::EmbeddingGenerator;

use anyhow::Result;
use std::sync::Arc;

/// Factory function for convenience - creates an embedding generator with registry
pub async fn create_embedding_generator(config: &crate::config::EmbeddingsConfig) -> Result<EmbeddingGenerator> {
    // Use async version for proper OpenAI support
    EmbeddingGenerator::new_async(config).await
}

/// Create a provider registry from enhanced configuration
pub async fn create_provider_registry(config: &EnhancedEmbeddingsConfig) -> Result<Arc<ProviderRegistry>> {
    Ok(Arc::new(ProviderRegistry::new(config).await?))
}

/// Create enhanced config from legacy config
pub fn create_enhanced_config(legacy_config: &crate::config::EmbeddingsConfig) -> EnhancedEmbeddingsConfig {
    EnhancedEmbeddingsConfig::from_legacy(
        legacy_config.model.clone(),
        legacy_config.batch_size
    )
}