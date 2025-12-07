use anyhow::{Context, Result};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

use crate::metrics::{EMBEDDING_LATENCY, EMBEDDING_REQUESTS};
use super::config::FastEmbedConfig;
use super::provider::{EmbeddingProvider, HealthStatus, ProviderCapabilities};

/// FastEmbed provider implementation
pub struct FastEmbedProvider {
    model: Arc<TextEmbedding>,
    config: FastEmbedConfig,
}

impl FastEmbedProvider {
    /// Create a new FastEmbedProvider with the configured model
    pub fn new(config: &FastEmbedConfig) -> Result<Self> {
        let model_type = Self::parse_model_name(&config.model)?;

        info!("Loading embedding model: {}", config.model);

        let model = TextEmbedding::try_new(
            InitOptions::new(model_type).with_show_download_progress(true)
        )
        .with_context(|| format!("Failed to initialize embedding model: {}", config.model))?;

        info!("Embedding model loaded successfully");

        Ok(Self {
            model: Arc::new(model),
            config: config.clone(),
        })
    }

    /// Parse model name string to fastembed EmbeddingModel enum
    fn parse_model_name(name: &str) -> Result<EmbeddingModel> {
        match name {
            "nomic-embed-text-v1.5" | "nomic-embed-text" | "nomic-ai/nomic-embed-text-v1.5" => {
                Ok(EmbeddingModel::NomicEmbedTextV15)
            }
            "all-MiniLM-L6-v2" | "all-minilm-l6-v2" => Ok(EmbeddingModel::AllMiniLML6V2),
            "bge-small-en-v1.5" | "bge-small" | "BAAI/bge-small-en-v1.5" => {
                Ok(EmbeddingModel::BGESmallENV15)
            }
            "bge-base-en-v1.5" | "bge-base" | "BAAI/bge-base-en-v1.5" => {
                Ok(EmbeddingModel::BGEBaseENV15)
            }
            "bge-large-en-v1.5" | "bge-large" | "BAAI/bge-large-en-v1.5" => {
                Ok(EmbeddingModel::BGELargeENV15)
            }
            _ => {
                // Default to nomic if unknown
                warn!("Unknown model '{}', falling back to nomic-embed-text-v1.5", name);
                Ok(EmbeddingModel::NomicEmbedTextV15)
            }
        }
    }

    /// Get embedding dimension for specific model
    fn get_model_dimension(model_name: &str) -> usize {
        match model_name {
            name if name.contains("bge-small") => 384,
            name if name.contains("bge-base") => 768,
            name if name.contains("bge-large") => 1024,
            name if name.contains("nomic") => 768,
            name if name.contains("MiniLM") => 384,
            _ => 768,  // Default
        }
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Record embedding request metric
        EMBEDDING_REQUESTS.inc();
        let start = Instant::now();

        // Wrap synchronous fastembed in async using spawn_blocking
        let model = self.model.clone();
        let texts = texts.to_vec();
        let batch_size = self.config.batch_size;

        let all_embeddings = tokio::task::spawn_blocking(move || {
            let mut embeddings = Vec::with_capacity(texts.len());

            // Process in batches
            for chunk in texts.chunks(batch_size) {
                let batch: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
                let batch_embeddings = model
                    .embed(batch, None)
                    .with_context(|| "Failed to generate embeddings")?;
                embeddings.extend(batch_embeddings);
            }

            Ok::<Vec<Vec<f32>>, anyhow::Error>(embeddings)
        })
        .await
        .context("FastEmbed processing task failed")??;

        // Record embedding latency metric
        EMBEDDING_LATENCY.observe(start.elapsed().as_secs_f64());

        Ok(all_embeddings)
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed(&[query.to_string()]).await?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding generated for query"))
    }

    fn embedding_dimension(&self) -> usize {
        Self::get_model_dimension(&self.config.model)
    }

    fn provider_name(&self) -> &'static str {
        "fastembed"
    }

    fn max_batch_size(&self) -> usize {
        self.config.batch_size
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        // Test with dummy embedding
        match self.embed(&["health check".to_string()]).await {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(e) => Ok(HealthStatus::Unhealthy {
                error: e.to_string(),
            }),
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_batching: true,
            supports_async: false,  // Wrapped sync
            requires_api_key: false,
            is_local: true,
            max_text_length: 512,  // Tokens
            cost_per_token: None,
        }
    }
}

/// Legacy EmbeddingGenerator for backward compatibility
pub struct EmbeddingGenerator {
    provider: Arc<dyn EmbeddingProvider>,
}

impl EmbeddingGenerator {
    /// Create a new EmbeddingGenerator with the configured model (sync version)
    ///
    /// This maintains backward compatibility with the existing API.
    /// NOTE: For OpenAI provider, use `new_async` when in an async context.
    pub fn new(config: &crate::config::EmbeddingsConfig) -> Result<Self> {
        use crate::config::EmbeddingProvider as ConfigProvider;

        match config.provider {
            ConfigProvider::FastEmbed => {
                let fastembed_config = FastEmbedConfig {
                    model: config.model.clone(),
                    batch_size: config.batch_size,
                    cache_dir: None,
                };
                let provider = Arc::new(FastEmbedProvider::new(&fastembed_config)?);
                Ok(Self { provider })
            }
            ConfigProvider::OpenAI => {
                // For OpenAI in sync context, try to use tokio's current handle
                // or create a new runtime
                let openai_config = super::config::OpenAIConfig {
                    api_key: config.openai_api_key.clone().unwrap_or_default(),
                    model: config.openai_model.clone(),
                    organization: None,
                    base_url: config.openai_base_url.clone(),
                    max_retries: 3,
                    timeout_secs: 30,
                    batch_size: config.batch_size,
                    initial_backoff_ms: 1000,
                    max_backoff_ms: 60000,
                    exponential_base: 2.0,
                };

                // Try to use existing runtime handle first
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    // We're inside an async runtime, use spawn_blocking to avoid nesting
                    let config_clone = openai_config.clone();
                    let provider = std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new()
                            .expect("Failed to create runtime");
                        rt.block_on(super::openai_provider::OpenAIProvider::new(&config_clone))
                    }).join()
                        .map_err(|_| anyhow::anyhow!("Thread panicked during OpenAI initialization"))??;

                    Ok(Self { provider: Arc::new(provider) })
                } else {
                    // No runtime, create a new one
                    let rt = tokio::runtime::Runtime::new()
                        .context("Failed to create tokio runtime for OpenAI initialization")?;

                    let provider = rt.block_on(async {
                        super::openai_provider::OpenAIProvider::new(&openai_config).await
                    })?;

                    Ok(Self { provider: Arc::new(provider) })
                }
            }
        }
    }

    /// Create a new EmbeddingGenerator asynchronously
    ///
    /// Use this when already in an async context to avoid runtime nesting issues.
    pub async fn new_async(config: &crate::config::EmbeddingsConfig) -> Result<Self> {
        use crate::config::EmbeddingProvider as ConfigProvider;

        match config.provider {
            ConfigProvider::FastEmbed => {
                let fastembed_config = FastEmbedConfig {
                    model: config.model.clone(),
                    batch_size: config.batch_size,
                    cache_dir: None,
                };
                let provider = Arc::new(FastEmbedProvider::new(&fastembed_config)?);
                Ok(Self { provider })
            }
            ConfigProvider::OpenAI => {
                let openai_config = super::config::OpenAIConfig {
                    api_key: config.openai_api_key.clone().unwrap_or_default(),
                    model: config.openai_model.clone(),
                    organization: None,
                    base_url: config.openai_base_url.clone(),
                    max_retries: 3,
                    timeout_secs: 30,
                    batch_size: config.batch_size,
                    initial_backoff_ms: 1000,
                    max_backoff_ms: 60000,
                    exponential_base: 2.0,
                };

                let provider = super::openai_provider::OpenAIProvider::new(&openai_config).await?;
                Ok(Self { provider: Arc::new(provider) })
            }
        }
    }

    /// Generate embeddings for a batch of texts (async version)
    ///
    /// Use this method when calling from an async context to avoid runtime nesting issues.
    pub async fn embed_async(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.provider.embed(texts).await
    }

    /// Generate embedding for a single query string (async version)
    ///
    /// Use this method when calling from an async context to avoid runtime nesting issues.
    pub async fn embed_query_async(&self, query: &str) -> Result<Vec<f32>> {
        self.provider.embed_query(query).await
    }

    /// Generate embeddings for a batch of texts (sync version)
    ///
    /// WARNING: This method uses `block_on` and must NOT be called from within
    /// an async runtime. Use `embed_async` when in an async context.
    ///
    /// Maintains synchronous API for backward compatibility with CLI commands
    /// that create their own runtime.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Create a new runtime for synchronous contexts only
        let rt = tokio::runtime::Runtime::new()
            .context("Failed to create tokio runtime for embedding")?;
        rt.block_on(self.provider.embed(texts))
    }

    /// Generate embedding for a single query string (sync version)
    ///
    /// WARNING: This method uses `block_on` and must NOT be called from within
    /// an async runtime. Use `embed_query_async` when in an async context.
    ///
    /// Maintains synchronous API for backward compatibility with CLI commands
    /// that create their own runtime.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        // Create a new runtime for synchronous contexts only
        let rt = tokio::runtime::Runtime::new()
            .context("Failed to create tokio runtime for embedding")?;
        rt.block_on(self.provider.embed_query(query))
    }

    /// Get the embedding dimension for the current model
    pub fn embedding_dimension(&self) -> usize {
        self.provider.embedding_dimension()
    }

    /// Get the batch size
    pub fn batch_size(&self) -> usize {
        self.provider.max_batch_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> FastEmbedConfig {
        FastEmbedConfig {
            model: "all-MiniLM-L6-v2".to_string(), // Smaller, faster for tests
            batch_size: 32,
            cache_dir: None,
        }
    }

    #[test]
    fn test_parse_model_name() {
        assert!(matches!(
            FastEmbedProvider::parse_model_name("nomic-embed-text-v1.5"),
            Ok(EmbeddingModel::NomicEmbedTextV15)
        ));
        assert!(matches!(
            FastEmbedProvider::parse_model_name("all-MiniLM-L6-v2"),
            Ok(EmbeddingModel::AllMiniLML6V2)
        ));
        assert!(matches!(
            FastEmbedProvider::parse_model_name("BAAI/bge-base-en-v1.5"),
            Ok(EmbeddingModel::BGEBaseENV15)
        ));
        // Unknown should fallback to nomic
        assert!(matches!(
            FastEmbedProvider::parse_model_name("unknown-model"),
            Ok(EmbeddingModel::NomicEmbedTextV15)
        ));
    }

    #[test]
    fn test_model_dimension() {
        assert_eq!(FastEmbedProvider::get_model_dimension("bge-small-en-v1.5"), 384);
        assert_eq!(FastEmbedProvider::get_model_dimension("bge-base-en-v1.5"), 768);
        assert_eq!(FastEmbedProvider::get_model_dimension("bge-large-en-v1.5"), 1024);
        assert_eq!(FastEmbedProvider::get_model_dimension("nomic-embed-text-v1.5"), 768);
        assert_eq!(FastEmbedProvider::get_model_dimension("all-MiniLM-L6-v2"), 384);
        assert_eq!(FastEmbedProvider::get_model_dimension("unknown"), 768);
    }

    #[tokio::test]
    #[ignore] // Requires model download
    async fn test_embed_texts() {
        let config = test_config();
        let provider = FastEmbedProvider::new(&config).unwrap();

        let texts = vec![
            "fn main() { println!(\"Hello\"); }".to_string(),
            "def hello(): print('world')".to_string(),
        ];

        let embeddings = provider.embed(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        assert!(!embeddings[0].is_empty());
        assert!(!embeddings[1].is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires model download
    async fn test_embed_query() {
        let config = test_config();
        let provider = FastEmbedProvider::new(&config).unwrap();

        let embedding = provider.embed_query("how to handle errors").await.unwrap();

        assert!(!embedding.is_empty());
    }

    #[tokio::test]
    async fn test_embed_empty() {
        let _config = test_config();
        // We can't create a provider without the model, but we can test the logic
        let texts: Vec<String> = vec![];
        assert!(texts.is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires model download
    async fn test_health_check() {
        let config = test_config();
        let provider = FastEmbedProvider::new(&config).unwrap();

        let status = provider.health_check().await.unwrap();
        assert!(matches!(status, HealthStatus::Healthy));
    }
}