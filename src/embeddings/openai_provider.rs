use anyhow::{anyhow, Context, Result};
use async_openai::{
    Client,
    config::OpenAIConfig as AsyncOpenAIConfig,
    types::CreateEmbeddingRequestArgs,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::metrics::{EMBEDDING_LATENCY, EMBEDDING_REQUESTS};
use super::config::OpenAIConfig;
use super::provider::{EmbeddingProvider, HealthStatus, ProviderCapabilities};

/// Rate limiter for API calls
struct RateLimiter {
    tokens: Arc<RwLock<f64>>,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: Arc<RwLock<Instant>>,
}

impl RateLimiter {
    fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(max_tokens)),
            max_tokens,
            refill_rate,
            last_refill: Arc::new(RwLock::new(Instant::now())),
        }
    }

    async fn acquire(&self, count: usize) -> Result<()> {
        loop {
            let mut tokens = self.tokens.write().await;
            let mut last_refill = self.last_refill.write().await;

            // Refill tokens
            let elapsed = last_refill.elapsed().as_secs_f64();
            *tokens = (*tokens + elapsed * self.refill_rate).min(self.max_tokens);
            *last_refill = Instant::now();

            if *tokens >= count as f64 {
                *tokens -= count as f64;
                return Ok(());
            }

            // Wait for tokens
            let wait_time = ((count as f64 - *tokens) / self.refill_rate) * 1000.0;
            drop(tokens);
            drop(last_refill);

            tokio::time::sleep(Duration::from_millis(wait_time as u64)).await;
        }
    }
}

/// OpenAI embedding provider implementation
pub struct OpenAIProvider {
    client: Client<AsyncOpenAIConfig>,
    config: OpenAIConfig,
    rate_limiter: Arc<RateLimiter>,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider
    pub async fn new(config: &OpenAIConfig) -> Result<Self> {
        let api_key = config.load_api_key()
            .context("Failed to load OpenAI API key")?;

        let mut openai_config = AsyncOpenAIConfig::new()
            .with_api_key(api_key);

        if let Some(org) = &config.organization {
            openai_config = openai_config.with_org_id(org);
        }

        if let Some(base_url) = &config.base_url {
            openai_config = openai_config.with_api_base(base_url);
        }

        let client = Client::with_config(openai_config);

        // Rate limiter: 3500 requests per minute
        let rate_limiter = Arc::new(RateLimiter::new(3500.0, 3500.0 / 60.0));

        info!("Initialized OpenAI provider with model: {}", config.model);

        Ok(Self {
            client,
            config: config.clone(),
            rate_limiter,
        })
    }

    /// Get model string for OpenAI API
    #[allow(dead_code)]
    fn get_model_string(name: &str) -> String {
        // Just return the string as-is since async-openai accepts strings now
        name.to_string()
    }

    /// Get embedding dimension for specific model
    fn get_model_dimension(model_name: &str) -> usize {
        match model_name {
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            "text-embedding-ada-002" => 1536,
            _ => 1536,  // Default
        }
    }

    /// Estimate token count for texts
    fn estimate_tokens(&self, texts: &[String]) -> usize {
        // Rough estimation: 1 token ≈ 4 characters
        texts.iter().map(|t| t.len().div_ceil(4)).sum()
    }

    /// Retry with exponential backoff
    async fn retry_with_backoff<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut attempt = 0;
        let mut backoff = self.config.initial_backoff_ms;

        loop {
            match f().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt >= self.config.max_retries => {
                    return Err(e).context("Max retries exceeded");
                }
                Err(e) => {
                    warn!("OpenAI request failed (attempt {}): {}", attempt + 1, e);
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                    backoff = (backoff as f64 * self.config.exponential_base) as u64;
                    backoff = backoff.min(self.config.max_backoff_ms);
                    attempt += 1;
                }
            }
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Record embedding request metric
        EMBEDDING_REQUESTS.inc();
        let start = Instant::now();

        // Rate limiting
        self.rate_limiter.acquire(texts.len()).await?;

        // Batch processing for API limits
        let mut all_embeddings = Vec::new();

        for batch in texts.chunks(self.config.batch_size.min(2048)) {
            let request = CreateEmbeddingRequestArgs::default()
                .model(&self.config.model)
                .input(batch.to_vec())
                .build()
                .context("Failed to build OpenAI request")?;

            // Retry logic with exponential backoff
            let response = self.retry_with_backoff(|| async {
                self.client
                    .embeddings()
                    .create(request.clone())
                    .await
                    .context("OpenAI API request failed")
            }).await?;

            for embedding_data in response.data {
                all_embeddings.push(embedding_data.embedding);
            }

            // Track token usage
            let tokens = self.estimate_tokens(batch);
            info!("Processed {} texts, estimated {} tokens", batch.len(), tokens);
        }

        // Record embedding latency metric
        EMBEDDING_LATENCY.observe(start.elapsed().as_secs_f64());

        Ok(all_embeddings)
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        // Record embedding request metric
        EMBEDDING_REQUESTS.inc();
        let start = Instant::now();

        self.rate_limiter.acquire(1).await?;

        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.config.model)
            .input(vec![query.to_string()])
            .build()
            .context("Failed to build query request")?;

        let response = self.retry_with_backoff(|| async {
            self.client
                .embeddings()
                .create(request.clone())
                .await
                .context("OpenAI API request failed")
        }).await?;

        // Record embedding latency metric
        EMBEDDING_LATENCY.observe(start.elapsed().as_secs_f64());

        response.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow!("No embedding returned"))
    }

    fn embedding_dimension(&self) -> usize {
        Self::get_model_dimension(&self.config.model)
    }

    fn provider_name(&self) -> &'static str {
        "openai"
    }

    fn max_batch_size(&self) -> usize {
        self.config.batch_size.min(2048)  // OpenAI limit
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        // Check API key validity with minimal request
        match self.embed_query("test").await {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(e) if e.to_string().contains("rate_limit") => {
                Ok(HealthStatus::Degraded {
                    reason: "Rate limited".to_string(),
                })
            }
            Err(e) => Ok(HealthStatus::Unhealthy {
                error: e.to_string(),
            }),
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_batching: true,
            supports_async: true,
            requires_api_key: true,
            is_local: false,
            max_text_length: 8191,  // Tokens
            cost_per_token: Some(match self.config.model.as_str() {
                "text-embedding-3-small" => 0.00002,  // $0.02 per 1M tokens
                "text-embedding-3-large" => 0.00013,  // $0.13 per 1M tokens
                _ => 0.00010,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn test_config() -> OpenAIConfig {
        OpenAIConfig {
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
        }
    }

    #[test]
    fn test_model_dimension() {
        assert_eq!(OpenAIProvider::get_model_dimension("text-embedding-3-small"), 1536);
        assert_eq!(OpenAIProvider::get_model_dimension("text-embedding-3-large"), 3072);
        assert_eq!(OpenAIProvider::get_model_dimension("text-embedding-ada-002"), 1536);
        assert_eq!(OpenAIProvider::get_model_dimension("unknown"), 1536);
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(10.0, 10.0);

        // Should succeed immediately
        assert!(limiter.acquire(5).await.is_ok());

        // Should succeed with remaining tokens
        assert!(limiter.acquire(5).await.is_ok());

        // Should wait for refill
        let start = Instant::now();
        limiter.acquire(5).await.unwrap();
        let elapsed = start.elapsed();

        // Should have waited for refill
        assert!(elapsed.as_millis() > 0);
    }

    #[tokio::test]
    #[ignore]  // Requires API key
    async fn test_openai_provider() {
        let config = OpenAIConfig {
            api_key: std::env::var("OPENAI_API_KEY").unwrap(),
            model: "text-embedding-3-small".to_string(),
            organization: None,
            base_url: None,
            max_retries: 3,
            timeout_secs: 30,
            batch_size: 10,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            exponential_base: 2.0,
        };

        let provider = OpenAIProvider::new(&config).await.unwrap();

        let query = "What is Rust programming?";
        let embedding = provider.embed_query(query).await.unwrap();

        assert_eq!(embedding.len(), 1536);  // text-embedding-3-small dimension
    }
}