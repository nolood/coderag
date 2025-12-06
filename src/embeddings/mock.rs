use anyhow::Result;
use async_trait::async_trait;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::provider::{EmbeddingProvider, ProviderCapabilities, HealthStatus};

/// Mock embedding provider for testing
pub struct MockEmbedder {
    dimension: usize,
}

impl MockEmbedder {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    fn text_to_vector(&self, text: &str) -> Vec<f32> {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Generate deterministic vector from hash
        let mut vector = Vec::with_capacity(self.dimension);
        let mut seed = hash;

        for _ in 0..self.dimension {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let value = ((seed / 65536) % 1000) as f32 / 1000.0;
            vector.push(value);
        }

        // Normalize vector
        let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for v in vector.iter_mut() {
                *v /= magnitude;
            }
        }

        vector
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|t| self.text_to_vector(t))
            .collect())
    }

    async fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        Ok(self.text_to_vector(query))
    }

    fn embedding_dimension(&self) -> usize {
        self.dimension
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }

    fn max_batch_size(&self) -> usize {
        1000
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus::Healthy)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_batching: true,
            supports_async: true,
            requires_api_key: false,
            is_local: true,
            max_text_length: 10000,
            cost_per_token: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_embedder_deterministic() {
        let embedder = MockEmbedder::new(768);

        let text = "test text";
        let vec1 = embedder.embed_query(text).await.unwrap();
        let vec2 = embedder.embed_query(text).await.unwrap();

        assert_eq!(vec1, vec2, "Same text should produce same vector");
    }

    #[tokio::test]
    async fn test_mock_embedder_dimension() {
        let embedder = MockEmbedder::new(512);
        let vec = embedder.embed_query("test").await.unwrap();

        assert_eq!(vec.len(), 512);
        assert_eq!(embedder.embedding_dimension(), 512);
    }

    #[tokio::test]
    async fn test_mock_embedder_normalized() {
        let embedder = MockEmbedder::new(768);
        let vec = embedder.embed_query("test").await.unwrap();

        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 1e-6, "Vector should be normalized");
    }
}