use anyhow::Result;
use async_trait::async_trait;

/// Core trait for embedding providers
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for multiple texts
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Generate embedding for a single query (may have special optimization)
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>>;

    /// Get the dimension of embeddings produced by this provider
    fn embedding_dimension(&self) -> usize;

    /// Get provider name for logging and metrics
    fn provider_name(&self) -> &'static str;

    /// Get maximum batch size this provider supports
    fn max_batch_size(&self) -> usize;

    /// Check if provider is healthy and ready
    async fn health_check(&self) -> Result<HealthStatus>;

    /// Get provider capabilities
    fn capabilities(&self) -> ProviderCapabilities;
}

/// Provider capabilities for feature detection
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub supports_batching: bool,
    pub supports_async: bool,
    pub requires_api_key: bool,
    pub is_local: bool,
    pub max_text_length: usize,
    pub cost_per_token: Option<f64>,
}

/// Health status for provider monitoring
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { error: String },
}

/// Provider information for registry management
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub is_active: bool,
    pub is_healthy: bool,
    pub capabilities: ProviderCapabilities,
}

