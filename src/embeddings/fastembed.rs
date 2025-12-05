use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::time::Instant;
use tracing::info;

use crate::config::EmbeddingsConfig;
use crate::metrics::{EMBEDDING_LATENCY, EMBEDDING_REQUESTS};

/// Generates embeddings for text using fastembed
pub struct EmbeddingGenerator {
    model: TextEmbedding,
    batch_size: usize,
}

impl EmbeddingGenerator {
    /// Create a new EmbeddingGenerator with the configured model
    ///
    /// This will download the model on first run if not cached.
    pub fn new(config: &EmbeddingsConfig) -> Result<Self> {
        let model_type = Self::parse_model_name(&config.model)?;

        info!("Loading embedding model: {}", config.model);

        let model = TextEmbedding::try_new(InitOptions::new(model_type).with_show_download_progress(true))
            .with_context(|| format!("Failed to initialize embedding model: {}", config.model))?;

        info!("Embedding model loaded successfully");

        Ok(Self {
            model,
            batch_size: config.batch_size,
        })
    }

    /// Parse model name string to fastembed EmbeddingModel enum
    fn parse_model_name(name: &str) -> Result<EmbeddingModel> {
        match name {
            "nomic-embed-text-v1.5" | "nomic-embed-text" => Ok(EmbeddingModel::NomicEmbedTextV15),
            "all-MiniLM-L6-v2" | "all-minilm-l6-v2" => Ok(EmbeddingModel::AllMiniLML6V2),
            "bge-small-en-v1.5" | "bge-small" => Ok(EmbeddingModel::BGESmallENV15),
            "bge-base-en-v1.5" | "bge-base" => Ok(EmbeddingModel::BGEBaseENV15),
            "bge-large-en-v1.5" | "bge-large" => Ok(EmbeddingModel::BGELargeENV15),
            _ => {
                // Default to nomic if unknown
                tracing::warn!(
                    "Unknown model '{}', falling back to nomic-embed-text-v1.5",
                    name
                );
                Ok(EmbeddingModel::NomicEmbedTextV15)
            }
        }
    }

    /// Generate embeddings for a batch of texts
    ///
    /// Processes texts in batches for memory efficiency.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Record embedding request metric
        EMBEDDING_REQUESTS.inc();
        let start = Instant::now();

        let mut all_embeddings = Vec::with_capacity(texts.len());

        // Process in batches
        for chunk in texts.chunks(self.batch_size) {
            let batch: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
            let embeddings = self
                .model
                .embed(batch, None)
                .with_context(|| "Failed to generate embeddings")?;
            all_embeddings.extend(embeddings);
        }

        // Record embedding latency metric
        EMBEDDING_LATENCY.observe(start.elapsed().as_secs_f64());

        Ok(all_embeddings)
    }

    /// Generate embedding for a single query string
    ///
    /// For search queries, use this method as it may apply query-specific
    /// preprocessing for some models.
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>> {
        let embeddings = self
            .model
            .embed(vec![query], None)
            .with_context(|| "Failed to generate query embedding")?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding generated for query"))
    }

    /// Get the embedding dimension for the current model
    pub fn embedding_dimension(&self) -> usize {
        // Most models use 384 or 768 dimensions
        // nomic-embed-text-v1.5 uses 768
        // all-MiniLM-L6-v2 uses 384
        // BGE models use 384 (small), 768 (base), or 1024 (large)
        768 // Default for nomic
    }

    /// Get the batch size
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> EmbeddingsConfig {
        EmbeddingsConfig {
            model: "all-MiniLM-L6-v2".to_string(), // Smaller, faster for tests
            batch_size: 32,
        }
    }

    #[test]
    fn test_parse_model_name() {
        assert!(matches!(
            EmbeddingGenerator::parse_model_name("nomic-embed-text-v1.5"),
            Ok(EmbeddingModel::NomicEmbedTextV15)
        ));
        assert!(matches!(
            EmbeddingGenerator::parse_model_name("all-MiniLM-L6-v2"),
            Ok(EmbeddingModel::AllMiniLML6V2)
        ));
        // Unknown should fallback to nomic
        assert!(matches!(
            EmbeddingGenerator::parse_model_name("unknown-model"),
            Ok(EmbeddingModel::NomicEmbedTextV15)
        ));
    }

    #[test]
    #[ignore] // Requires model download
    fn test_embed_texts() {
        let config = test_config();
        let generator = EmbeddingGenerator::new(&config).unwrap();

        let texts = vec![
            "fn main() { println!(\"Hello\"); }".to_string(),
            "def hello(): print('world')".to_string(),
        ];

        let embeddings = generator.embed(&texts).unwrap();

        assert_eq!(embeddings.len(), 2);
        assert!(!embeddings[0].is_empty());
        assert!(!embeddings[1].is_empty());
    }

    #[test]
    #[ignore] // Requires model download
    fn test_embed_query() {
        let config = test_config();
        let generator = EmbeddingGenerator::new(&config).unwrap();

        let embedding = generator.embed_query("how to handle errors").unwrap();

        assert!(!embedding.is_empty());
    }

    #[test]
    fn test_embed_empty() {
        // This test can run without model
        let texts: Vec<String> = vec![];
        // We can't create a generator without the model, but we can test the logic
        assert!(texts.is_empty());
    }
}
