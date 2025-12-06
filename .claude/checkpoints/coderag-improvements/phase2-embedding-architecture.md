# Phase 2: Embedding Provider Architecture Specification

**Date:** 2025-12-06
**Purpose:** Complete architectural design for embedding provider abstraction system
**Status:** Design Phase

---

## Executive Summary

This document specifies the architecture for abstracting embedding providers in CodeRAG, enabling runtime selection between FastEmbed (local) and OpenAI (API-based) providers. The design maintains backward compatibility while introducing a flexible, extensible provider system aligned with Domain-Driven Design principles.

---

## 1. Architecture Overview

### 1.1 High-Level Design Decisions

**Pattern Choice**: Provider Pattern with Dynamic Dispatch
- **Why**: Enables runtime provider selection without compile-time coupling
- **Trade-off**: Small runtime overhead vs. flexibility and testability

**Async-First Design**: All providers implement async trait
- **Why**: OpenAI requires async; FastEmbed can be wrapped in async
- **Trade-off**: Slight overhead for sync providers vs. unified interface

**Registry Pattern**: Centralized provider management
- **Why**: Consistent with codebase patterns (ExtractorRegistry)
- **Trade-off**: Additional abstraction layer vs. direct instantiation

**Configuration-Driven**: TOML-based provider selection
- **Why**: Runtime flexibility without code changes
- **Trade-off**: Configuration complexity vs. hardcoded simplicity

### 1.2 System Architecture Layers

```
┌─────────────────────────────────────────────────────┐
│                 Presentation Layer                  │
│         (CLI commands, API endpoints)               │
└──────────────────┬──────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────┐
│                Application Layer                    │
│    (SearchEngine, Indexer - existing consumers)     │
└──────────────────┬──────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────┐
│                 Domain Layer                        │
│  ┌──────────────────────────────────────────────┐  │
│  │          EmbeddingGenerator (Facade)         │  │
│  │         Maintains backward compatibility     │  │
│  └──────────────────┬───────────────────────────┘  │
│                     │                               │
│  ┌──────────────────▼───────────────────────────┐  │
│  │          EmbeddingProvider (Trait)           │  │
│  │      Abstract interface for all providers    │  │
│  └───────┬──────────────────┬───────────────────┘  │
│          │                  │                       │
│  ┌───────▼────────┐ ┌──────▼──────────┐          │
│  │FastEmbedProvider│ │ OpenAIProvider  │          │
│  └────────────────┘ └─────────────────┘          │
└─────────────────────────────────────────────────────┘
                   │
┌──────────────────▼──────────────────────────────────┐
│               Infrastructure Layer                  │
│  (HTTP clients, model loading, API connections)     │
└─────────────────────────────────────────────────────┘
```

### 1.3 Data Flow Architecture

```
User Query
    │
    ▼
SearchEngine.search()
    │
    ▼
EmbeddingGenerator.embed_query()
    │
    ▼
ProviderRegistry.get_current()
    │
    ├─→ FastEmbedProvider
    │     ├─→ Load local model
    │     └─→ Generate embeddings
    │
    └─→ OpenAIProvider
          ├─→ Build HTTP request
          ├─→ Call OpenAI API
          └─→ Parse response

Embedding Vector (Vec<f32>)
    │
    ▼
VectorDB Query
```

---

## 2. Domain-Driven Design

### 2.1 Bounded Contexts

**Embedding Context**
- **Purpose**: Generate vector representations of text
- **Core Domain**: Text-to-vector transformation
- **Aggregates**: EmbeddingSession, EmbeddingBatch
- **Value Objects**: EmbeddingVector, ModelIdentifier, BatchConfiguration

**Provider Context**
- **Purpose**: Abstract embedding generation implementations
- **Supporting Domain**: Provider selection and management
- **Aggregates**: ProviderRegistry, ProviderConfiguration
- **Value Objects**: ProviderName, ApiCredentials, ModelSettings

### 2.2 Aggregate Design

**EmbeddingSession (Aggregate Root)**
```rust
pub struct EmbeddingSession {
    id: SessionId,
    provider: Arc<dyn EmbeddingProvider>,
    metrics: SessionMetrics,
    cache: Option<EmbeddingCache>,
}

// Invariants:
// - Session must have valid provider
// - Metrics must be initialized before first embedding
// - Cache invalidation must occur on provider change
```

**EmbeddingBatch (Entity)**
```rust
pub struct EmbeddingBatch {
    texts: Vec<String>,
    batch_size: usize,
    processing_strategy: BatchStrategy,
}

// Invariants:
// - Batch size must not exceed provider limits
// - Empty batches are not processed
// - Text preprocessing must be consistent
```

**EmbeddingVector (Value Object)**
```rust
#[derive(Clone, Debug)]
pub struct EmbeddingVector {
    dimensions: usize,
    values: Vec<f32>,
    normalized: bool,
}

// Invariants:
// - Vector dimension must match provider specification
// - Values must be finite (no NaN or Inf)
// - Normalized vectors must have L2 norm = 1.0
```

### 2.3 Domain Services

**EmbeddingService**
```rust
pub struct EmbeddingService {
    registry: Arc<ProviderRegistry>,
    cache: Arc<EmbeddingCache>,
    metrics: Arc<MetricsCollector>,
}

impl EmbeddingService {
    // Operations that don't belong to entities
    pub async fn embed_with_fallback(&self, texts: &[String]) -> Result<Vec<EmbeddingVector>>;
    pub async fn validate_provider_health(&self, provider: &str) -> Result<HealthStatus>;
    pub async fn estimate_cost(&self, texts: &[String], provider: &str) -> Result<CostEstimate>;
}
```

### 2.4 Domain Events

**Provider Events**
```rust
pub enum EmbeddingDomainEvent {
    // Provider lifecycle
    ProviderRegistered { name: String, model: String },
    ProviderActivated { name: String },
    ProviderDeactivated { name: String, reason: String },

    // Operational events
    BatchProcessed { provider: String, count: usize, duration: Duration },
    ProviderFailed { provider: String, error: String, fallback_used: bool },
    RateLimitReached { provider: String, retry_after: Duration },

    // Cost tracking
    ApiCostIncurred { provider: String, tokens: usize, cost: f64 },
}
```

**Event Handling Strategy**
- Events published to internal bus
- Metrics collector subscribes to operational events
- Cost tracker subscribes to API usage events
- Health monitor subscribes to failure events

---

## 3. Interface Definitions

### 3.1 Core Trait Definition

```rust
use async_trait::async_trait;
use anyhow::Result;

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
```

### 3.2 Provider Registry Interface

```rust
pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn EmbeddingProvider>>>,
    active_provider: RwLock<String>,
    fallback_chain: Vec<String>,
}

impl ProviderRegistry {
    /// Create new registry with default providers
    pub fn new(config: &EmbeddingsConfig) -> Result<Self>;

    /// Register a new provider
    pub async fn register(
        &self,
        name: String,
        provider: Arc<dyn EmbeddingProvider>
    ) -> Result<()>;

    /// Get the currently active provider
    pub async fn get_active(&self) -> Result<Arc<dyn EmbeddingProvider>>;

    /// Get a specific provider by name
    pub async fn get(&self, name: &str) -> Result<Arc<dyn EmbeddingProvider>>;

    /// Switch active provider
    pub async fn switch_provider(&self, name: &str) -> Result<()>;

    /// Get provider with automatic fallback on failure
    pub async fn get_with_fallback(&self) -> Result<Arc<dyn EmbeddingProvider>>;

    /// List all registered providers
    pub async fn list_providers(&self) -> Vec<ProviderInfo>;
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub is_active: bool,
    pub is_healthy: bool,
    pub capabilities: ProviderCapabilities,
}
```

### 3.3 Configuration Interfaces

```rust
/// Main embeddings configuration
#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingsConfig {
    /// Active provider name
    pub provider: String,

    /// Fallback providers in order of preference
    pub fallback_chain: Vec<String>,

    /// Provider-specific configurations
    pub providers: ProvidersConfig,

    /// Global settings
    pub cache: CacheConfig,
    pub retry: RetryConfig,
}

/// Provider-specific configurations
#[derive(Debug, Clone, Deserialize)]
pub struct ProvidersConfig {
    pub fastembed: Option<FastEmbedConfig>,
    pub openai: Option<OpenAIConfig>,
    // Extensible for future providers
}

/// FastEmbed provider configuration
#[derive(Debug, Clone, Deserialize)]
pub struct FastEmbedConfig {
    pub model: String,
    pub batch_size: usize,
    pub cache_dir: Option<PathBuf>,
}

/// OpenAI provider configuration
#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub model: String,
    pub organization: Option<String>,
    pub base_url: Option<String>,  // For Azure or custom endpoints
    pub max_retries: usize,
    pub timeout_secs: u64,
    pub batch_size: usize,
}

/// Cache configuration
#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub ttl_seconds: u64,
    pub max_entries: usize,
}

/// Retry configuration
#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub exponential_base: f64,
}
```

---

## 4. Provider Implementations

### 4.1 FastEmbedProvider Structure

```rust
pub struct FastEmbedProvider {
    model: Arc<TextEmbedding>,
    config: FastEmbedConfig,
    metrics: Arc<MetricsCollector>,
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Wrap synchronous fastembed in async
        let model = self.model.clone();
        let texts = texts.to_vec();

        tokio::task::spawn_blocking(move || {
            // Process in batches
            let embeddings = model.embed(texts, None)?;
            Ok(embeddings.into_iter().collect())
        })
        .await
        .context("FastEmbed processing failed")?
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.embed(&[query.to_string()])
            .await
            .map(|mut vecs| vecs.pop().unwrap())
    }

    fn embedding_dimension(&self) -> usize {
        match self.config.model.as_str() {
            "BAAI/bge-small-en-v1.5" => 384,
            "BAAI/bge-base-en-v1.5" => 768,
            "BAAI/bge-large-en-v1.5" => 1024,
            "nomic-ai/nomic-embed-text-v1.5" => 768,
            _ => 768,  // Default
        }
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
                error: e.to_string()
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
```

### 4.2 OpenAIProvider Structure

```rust
use async_openai::{Client, types::CreateEmbeddingRequestArgs};

pub struct OpenAIProvider {
    client: Client<async_openai::config::OpenAIConfig>,
    config: OpenAIConfig,
    metrics: Arc<MetricsCollector>,
    rate_limiter: Arc<RateLimiter>,
}

#[async_trait]
impl EmbeddingProvider for OpenAIProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Rate limiting
        self.rate_limiter.acquire(texts.len()).await?;

        // Batch processing for API limits
        let mut all_embeddings = Vec::new();

        for batch in texts.chunks(self.config.batch_size) {
            let request = CreateEmbeddingRequestArgs::default()
                .model(&self.config.model)
                .input(batch.to_vec())
                .build()
                .context("Failed to build OpenAI request")?;

            // Retry logic with exponential backoff
            let response = self.retry_with_backoff(|| async {
                self.client.embeddings().create(request.clone()).await
            }).await?;

            for embedding_data in response.data {
                all_embeddings.push(embedding_data.embedding);
            }

            // Track costs
            self.metrics.track_api_usage(
                batch.len(),
                self.estimate_tokens(batch)
            );
        }

        Ok(all_embeddings)
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        self.rate_limiter.acquire(1).await?;

        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.config.model)
            .input(vec![query.to_string()])
            .build()
            .context("Failed to build query request")?;

        let response = self.retry_with_backoff(|| async {
            self.client.embeddings().create(request.clone()).await
        }).await?;

        response.data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow!("No embedding returned"))
    }

    fn embedding_dimension(&self) -> usize {
        match self.config.model.as_str() {
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            "text-embedding-ada-002" => 1536,
            _ => 1536,  // Default
        }
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
                    reason: "Rate limited".to_string()
                })
            }
            Err(e) => Ok(HealthStatus::Unhealthy {
                error: e.to_string()
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

impl OpenAIProvider {
    async fn retry_with_backoff<F, Fut, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
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

    fn estimate_tokens(&self, texts: &[String]) -> usize {
        // Rough estimation: 1 token ≈ 4 characters
        texts.iter().map(|t| t.len() / 4).sum()
    }
}
```

---

## 5. Module Organization

### 5.1 Directory Structure

```
src/embeddings/
├── mod.rs                  # Public API and re-exports
├── provider.rs             # EmbeddingProvider trait definition
├── registry.rs             # ProviderRegistry implementation
├── config.rs               # Configuration structures
├── generators/
│   └── facade.rs           # EmbeddingGenerator facade
├── providers/
│   ├── mod.rs             # Provider module organization
│   ├── fastembed.rs       # FastEmbedProvider implementation
│   └── openai.rs          # OpenAIProvider implementation
├── cache/
│   ├── mod.rs             # Cache traits
│   └── memory.rs          # In-memory cache implementation
└── utils/
    ├── rate_limiter.rs    # Rate limiting for API providers
    ├── metrics.rs         # Provider-specific metrics
    └── retry.rs           # Retry logic utilities
```

### 5.2 Module Interfaces

**mod.rs (Public API)**
```rust
mod provider;
mod registry;
mod config;
mod generators;
mod providers;
mod cache;
mod utils;

// Re-export public interfaces
pub use provider::{EmbeddingProvider, ProviderCapabilities, HealthStatus};
pub use registry::{ProviderRegistry, ProviderInfo};
pub use config::{EmbeddingsConfig, ProvidersConfig};
pub use generators::facade::EmbeddingGenerator;

// Factory function for convenience
pub async fn create_embedding_generator(config: &EmbeddingsConfig) -> Result<EmbeddingGenerator> {
    let registry = ProviderRegistry::new(config)?;
    Ok(EmbeddingGenerator::new(registry))
}
```

**providers/mod.rs**
```rust
pub mod fastembed;
pub mod openai;

use super::provider::EmbeddingProvider;
use super::config::ProvidersConfig;

/// Factory for creating providers from configuration
pub struct ProviderFactory;

impl ProviderFactory {
    pub async fn create_provider(
        name: &str,
        config: &ProvidersConfig
    ) -> Result<Arc<dyn EmbeddingProvider>> {
        match name {
            "fastembed" => {
                let config = config.fastembed.as_ref()
                    .ok_or_else(|| anyhow!("FastEmbed config missing"))?;
                Ok(Arc::new(fastembed::FastEmbedProvider::new(config)?))
            }
            "openai" => {
                let config = config.openai.as_ref()
                    .ok_or_else(|| anyhow!("OpenAI config missing"))?;
                Ok(Arc::new(openai::OpenAIProvider::new(config).await?))
            }
            _ => Err(anyhow!("Unknown provider: {}", name)),
        }
    }
}
```

---

## 6. Configuration Schema

### 6.1 TOML Configuration Examples

**Default Configuration (FastEmbed only)**
```toml
[embeddings]
provider = "fastembed"
fallback_chain = []

[embeddings.providers.fastembed]
model = "nomic-ai/nomic-embed-text-v1.5"
batch_size = 32
cache_dir = ".cache/models"

[embeddings.cache]
enabled = true
ttl_seconds = 3600
max_entries = 10000

[embeddings.retry]
max_attempts = 3
initial_backoff_ms = 100
max_backoff_ms = 10000
exponential_base = 2.0
```

**OpenAI Primary with FastEmbed Fallback**
```toml
[embeddings]
provider = "openai"
fallback_chain = ["fastembed"]

[embeddings.providers.openai]
api_key = "${OPENAI_API_KEY}"  # Environment variable
model = "text-embedding-3-small"
organization = "org-xyz"
batch_size = 100
max_retries = 3
timeout_secs = 30

[embeddings.providers.fastembed]
model = "BAAI/bge-base-en-v1.5"
batch_size = 32

[embeddings.cache]
enabled = true
ttl_seconds = 7200
max_entries = 50000
```

**Azure OpenAI Configuration**
```toml
[embeddings]
provider = "openai"

[embeddings.providers.openai]
api_key = "${AZURE_OPENAI_KEY}"
model = "text-embedding-3-small"
base_url = "https://myresource.openai.azure.com"
batch_size = 50
```

### 6.2 Environment Variables

```bash
# OpenAI Configuration
OPENAI_API_KEY=sk-...
OPENAI_ORG_ID=org-...

# Azure OpenAI
AZURE_OPENAI_KEY=...
AZURE_OPENAI_ENDPOINT=https://...

# Override config file
CODERAG_EMBEDDING_PROVIDER=openai
CODERAG_EMBEDDING_MODEL=text-embedding-3-large
```

---

## 7. Migration Strategy

### 7.1 Backward Compatibility Layer

**Current API (Preserved)**
```rust
// Existing code continues to work
let embedder = EmbeddingGenerator::new(&config)?;
let embeddings = embedder.embed(&texts)?;
```

**Implementation with Provider Abstraction**
```rust
pub struct EmbeddingGenerator {
    registry: Arc<ProviderRegistry>,
    runtime: Handle,  // For async bridge
}

impl EmbeddingGenerator {
    /// Maintain existing synchronous API
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.runtime.block_on(async {
            let provider = self.registry.get_active().await?;
            provider.embed(texts).await
        })
    }

    /// New async API for better performance
    pub async fn embed_async(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let provider = self.registry.get_active().await?;
        provider.embed(texts).await
    }
}
```

### 7.2 Phased Migration Plan

**Phase 1: Infrastructure (Week 1)**
1. Implement provider trait and registry
2. Create provider implementations
3. Add configuration structures
4. Unit tests for each component

**Phase 2: Integration (Week 2)**
1. Update EmbeddingGenerator facade
2. Integrate with existing SearchEngine
3. Add metrics and monitoring
4. Integration tests

**Phase 3: Deployment (Week 3)**
1. Deploy with FastEmbed as default
2. Enable OpenAI for testing environments
3. Monitor performance and costs
4. Gradual rollout to production

**Phase 4: Optimization (Week 4)**
1. Implement caching layer
2. Add rate limiting for API providers
3. Optimize batch processing
4. Performance benchmarking

### 7.3 Rollback Strategy

```rust
// Feature flag for provider system
pub struct EmbeddingGenerator {
    use_legacy: bool,
    legacy_impl: Option<LegacyEmbedding>,
    provider_impl: Option<Arc<ProviderRegistry>>,
}

impl EmbeddingGenerator {
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if self.use_legacy {
            self.legacy_impl.as_ref().unwrap().embed(texts)
        } else {
            self.provider_impl.as_ref().unwrap().embed(texts)
        }
    }
}
```

---

## 8. Error Handling Strategy

### 8.1 Error Type Hierarchy

```rust
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Provider unavailable: {0}")]
    ProviderUnavailable(String),

    #[error("API error: {0}")]
    ApiError(#[from] ApiError),

    #[error("Rate limit exceeded, retry after {0:?}")]
    RateLimited(Duration),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Model loading failed: {0}")]
    ModelError(String),

    #[error("All providers failed")]
    AllProvidersFailed(Vec<String>),
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Request failed: {0}")]
    RequestError(String),

    #[error("Response parsing failed: {0}")]
    ParseError(String),

    #[error("Quota exceeded")]
    QuotaExceeded,
}
```

### 8.2 Fallback Strategy

```rust
impl ProviderRegistry {
    pub async fn embed_with_fallback(
        &self,
        texts: &[String]
    ) -> Result<Vec<Vec<f32>>> {
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

        Err(EmbeddingError::AllProvidersFailed(errors).into())
    }
}
```

---

## 9. Testing Strategy

### 9.1 Unit Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;
    use mockall::mock;

    // Mock provider for testing
    mock! {
        Provider {}

        #[async_trait]
        impl EmbeddingProvider for Provider {
            async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
            async fn embed_query(&self, query: &str) -> Result<Vec<f32>>;
            fn embedding_dimension(&self) -> usize;
            fn provider_name(&self) -> &'static str;
            fn max_batch_size(&self) -> usize;
            async fn health_check(&self) -> Result<HealthStatus>;
            fn capabilities(&self) -> ProviderCapabilities;
        }
    }

    #[tokio::test]
    async fn test_registry_fallback() {
        let mut primary = MockProvider::new();
        primary.expect_embed()
            .times(1)
            .returning(|_| Err(anyhow!("Primary failed")));

        let mut fallback = MockProvider::new();
        fallback.expect_embed()
            .times(1)
            .returning(|texts| Ok(vec![vec![1.0; 768]; texts.len()]));

        let mut registry = ProviderRegistry::new_empty();
        registry.register("primary", Arc::new(primary)).await.unwrap();
        registry.register("fallback", Arc::new(fallback)).await.unwrap();
        registry.fallback_chain = vec!["fallback".to_string()];

        let result = registry.embed_with_fallback(&["test".to_string()]).await;
        assert!(result.is_ok());
    }
}
```

### 9.2 Integration Test Patterns

```rust
#[tokio::test]
async fn test_fastembed_provider_integration() {
    let config = FastEmbedConfig {
        model: "BAAI/bge-small-en-v1.5".to_string(),
        batch_size: 2,
        cache_dir: None,
    };

    let provider = FastEmbedProvider::new(&config).unwrap();

    let texts = vec![
        "First test text".to_string(),
        "Second test text".to_string(),
    ];

    let embeddings = provider.embed(&texts).await.unwrap();

    assert_eq!(embeddings.len(), 2);
    assert_eq!(embeddings[0].len(), 384);  // bge-small dimension

    // Test normalization
    for embedding in &embeddings {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);  // Should be normalized
    }
}

#[tokio::test]
#[ignore]  // Requires API key
async fn test_openai_provider_integration() {
    let api_key = std::env::var("OPENAI_API_KEY").unwrap();

    let config = OpenAIConfig {
        api_key,
        model: "text-embedding-3-small".to_string(),
        organization: None,
        base_url: None,
        max_retries: 3,
        timeout_secs: 30,
        batch_size: 10,
    };

    let provider = OpenAIProvider::new(&config).await.unwrap();

    let query = "What is rust programming?";
    let embedding = provider.embed_query(query).await.unwrap();

    assert_eq!(embedding.len(), 1536);  // text-embedding-3-small dimension
}
```

### 9.3 Performance Benchmarks

```rust
#[bench]
fn bench_fastembed_batch_processing(b: &mut Bencher) {
    let runtime = Runtime::new().unwrap();
    let provider = create_fastembed_provider();

    let texts: Vec<String> = (0..100)
        .map(|i| format!("Test text number {}", i))
        .collect();

    b.iter(|| {
        runtime.block_on(provider.embed(&texts))
    });
}

#[bench]
fn bench_cache_hit_rate(b: &mut Bencher) {
    let cache = EmbeddingCache::new(1000);
    let texts = vec!["cached text".to_string()];

    // Warm up cache
    cache.get_or_insert(&texts, || vec![vec![1.0; 768]]);

    b.iter(|| {
        cache.get(&texts)
    });
}
```

---

## 10. Metrics and Monitoring

### 10.1 Metrics Definition

```rust
lazy_static! {
    // Provider-specific metrics
    pub static ref PROVIDER_REQUESTS: IntCounterVec = register_int_counter_vec!(
        "embedding_provider_requests_total",
        "Total embedding requests by provider",
        &["provider", "status"]
    ).unwrap();

    pub static ref PROVIDER_LATENCY: HistogramVec = register_histogram_vec!(
        "embedding_provider_latency_seconds",
        "Embedding latency by provider",
        &["provider", "operation"],
        exponential_buckets(0.001, 2.0, 12).unwrap()
    ).unwrap();

    pub static ref API_TOKENS_USED: IntCounterVec = register_int_counter_vec!(
        "embedding_api_tokens_total",
        "Total tokens used by API providers",
        &["provider"]
    ).unwrap();

    pub static ref API_COST_ESTIMATE: CounterVec = register_counter_vec!(
        "embedding_api_cost_dollars",
        "Estimated API cost in dollars",
        &["provider"]
    ).unwrap();

    pub static ref CACHE_HIT_RATE: GaugeVec = register_gauge_vec!(
        "embedding_cache_hit_rate",
        "Cache hit rate for embeddings",
        &["cache_type"]
    ).unwrap();

    pub static ref FALLBACK_TRIGGERED: IntCounter = register_int_counter!(
        "embedding_fallback_triggered_total",
        "Total number of fallback triggers"
    ).unwrap();
}
```

### 10.2 Observability Implementation

```rust
impl OpenAIProvider {
    async fn embed_with_metrics(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let timer = PROVIDER_LATENCY
            .with_label_values(&["openai", "embed"])
            .start_timer();

        let result = self.embed_internal(texts).await;

        match &result {
            Ok(embeddings) => {
                PROVIDER_REQUESTS
                    .with_label_values(&["openai", "success"])
                    .inc();

                // Track token usage
                let tokens = self.estimate_tokens(texts);
                API_TOKENS_USED
                    .with_label_values(&["openai"])
                    .inc_by(tokens as u64);

                // Track estimated cost
                let cost = tokens as f64 * self.cost_per_token();
                API_COST_ESTIMATE
                    .with_label_values(&["openai"])
                    .inc_by(cost);
            }
            Err(e) => {
                PROVIDER_REQUESTS
                    .with_label_values(&["openai", "error"])
                    .inc();

                error!("OpenAI embedding failed: {}", e);
            }
        }

        timer.observe_duration();
        result
    }
}
```

---

## 11. Architecture Trade-offs

### 11.1 Design Decisions and Rationale

| Decision | Choice | Alternative | Trade-off |
|----------|--------|-------------|-----------|
| **Provider Interface** | Async trait with dynamic dispatch | Generic types with static dispatch | Runtime flexibility vs compile-time optimization |
| **Configuration** | TOML with environment override | Code-based configuration | User flexibility vs type safety |
| **Caching** | In-memory LRU cache | Distributed cache (Redis) | Simplicity vs scalability |
| **Fallback** | Sequential chain | Parallel attempts | Predictable cost vs latency |
| **Batch Processing** | Provider-defined sizes | Global batch size | Provider optimization vs consistency |
| **Error Handling** | Explicit error types | Generic anyhow::Error | Detailed errors vs simplicity |
| **Metrics** | Prometheus format | Custom metrics | Industry standard vs specific needs |

### 11.2 Performance Considerations

**Memory Usage**
- FastEmbed: ~500MB for model loading
- OpenAI: Minimal (HTTP client only)
- Cache: Configurable, default 100MB

**Latency Characteristics**
- FastEmbed: 10-50ms per batch (local)
- OpenAI: 100-500ms per request (network)
- Cache hit: <1ms

**Throughput Limits**
- FastEmbed: CPU/GPU bound, ~1000 texts/sec
- OpenAI: Rate limited, ~3500 requests/min

### 11.3 Scalability Analysis

**Vertical Scaling**
- FastEmbed benefits from more CPU cores
- Memory usage scales with cache size
- Provider registry has minimal overhead

**Horizontal Scaling**
- Stateless design enables multiple instances
- Cache can be shared via external store
- Rate limiting needs coordination

**Cost Implications**
- FastEmbed: Infrastructure cost only
- OpenAI: $0.02-$0.13 per million tokens
- Hybrid approach optimizes cost/performance

---

## 12. Security Considerations

### 12.1 API Key Management

```rust
impl OpenAIConfig {
    pub fn load_api_key(&self) -> Result<String> {
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
            .context("No API key configured")
    }
}
```

### 12.2 Data Privacy

- No persistent storage of embeddings by default
- Cache entries expire based on TTL
- API requests use HTTPS
- Sensitive text can bypass cache

### 12.3 Rate Limiting Protection

```rust
pub struct RateLimiter {
    tokens: Arc<Mutex<f64>>,
    max_tokens: f64,
    refill_rate: f64,
    last_refill: Arc<Mutex<Instant>>,
}

impl RateLimiter {
    pub async fn acquire(&self, count: usize) -> Result<()> {
        loop {
            let mut tokens = self.tokens.lock().await;
            let mut last_refill = self.last_refill.lock().await;

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
```

---

## 13. Future Extensions

### 13.1 Additional Providers

**Planned Provider Support**
- Cohere embeddings
- Anthropic embeddings (when available)
- Local ONNX models
- Custom model servers

**Provider Interface Extension**
```rust
pub trait EmbeddingProviderExt: EmbeddingProvider {
    /// Support for multimodal embeddings
    async fn embed_multimodal(&self, inputs: &[MultimodalInput]) -> Result<Vec<Vec<f32>>>;

    /// Fine-tuning support
    async fn fine_tune(&self, dataset: &Dataset) -> Result<ModelCheckpoint>;

    /// Streaming embeddings for large batches
    fn embed_stream(&self, texts: &[String]) -> BoxStream<Result<Vec<f32>>>;
}
```

### 13.2 Advanced Features

**Semantic Caching**
```rust
pub struct SemanticCache {
    provider: Arc<dyn EmbeddingProvider>,
    threshold: f32,  // Similarity threshold
    index: VectorIndex,
}

impl SemanticCache {
    pub async fn get_similar(&self, text: &str) -> Option<Vec<f32>> {
        let query_embedding = self.provider.embed_query(text).await.ok()?;
        self.index.search_similar(&query_embedding, self.threshold)
    }
}
```

**Cross-Provider Ensemble**
```rust
pub struct EnsembleProvider {
    providers: Vec<Arc<dyn EmbeddingProvider>>,
    weights: Vec<f32>,
}

impl EnsembleProvider {
    pub async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Combine embeddings from multiple providers
        let futures: Vec<_> = self.providers.iter()
            .map(|p| p.embed(texts))
            .collect();

        let results = futures::future::join_all(futures).await;

        // Weighted average of embeddings
        self.combine_embeddings(results, &self.weights)
    }
}
```

---

## 14. Summary

This architecture provides a robust, extensible embedding provider system that:

1. **Maintains backward compatibility** with existing `EmbeddingGenerator` API
2. **Enables runtime provider selection** through configuration
3. **Supports both local and API-based** embedding generation
4. **Implements comprehensive error handling** with fallback chains
5. **Provides detailed metrics** for monitoring and cost tracking
6. **Follows Domain-Driven Design** principles for maintainability
7. **Scales horizontally and vertically** based on deployment needs
8. **Optimizes for both performance and cost** through intelligent caching

The design balances flexibility with simplicity, allowing CodeRAG to evolve its embedding capabilities while maintaining system stability and performance.

---

**Document Version**: 1.0
**Last Updated**: 2025-12-06
**Status**: Ready for Implementation
**Next Phase**: Implementation of core provider trait and registry