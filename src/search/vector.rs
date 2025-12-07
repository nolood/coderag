use anyhow::{Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

use super::traits::Search;
use crate::embeddings::EmbeddingGenerator;
use crate::metrics::{SEARCH_LATENCY, SEARCH_REQUESTS, SEARCH_RESULTS};
use crate::storage::Storage;

pub use crate::storage::SearchResult;

/// Search engine for semantic code search using vector embeddings.
///
/// This engine converts queries into embeddings and performs
/// similarity search against indexed code chunks.
pub struct SearchEngine {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
}

impl SearchEngine {
    /// Create a new SearchEngine with the given storage and embedder
    pub fn new(storage: Arc<Storage>, embedder: Arc<EmbeddingGenerator>) -> Self {
        Self { storage, embedder }
    }

    /// Search and deduplicate results by file
    ///
    /// Returns at most one result per file, the highest scoring chunk
    pub async fn search_unique_files(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Get more results to ensure we have enough unique files
        let results = self.search(query, limit * 3).await?;

        let mut seen_files = std::collections::HashSet::new();
        let unique_results: Vec<SearchResult> = results
            .into_iter()
            .filter(|r| seen_files.insert(r.file_path.clone()))
            .take(limit)
            .collect();

        Ok(unique_results)
    }

    /// Get a reference to the underlying storage
    pub fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }

    /// Get a reference to the embedder
    pub fn embedder(&self) -> &Arc<EmbeddingGenerator> {
        &self.embedder
    }
}

#[async_trait]
impl Search for SearchEngine {
    /// Perform semantic search for the given query
    ///
    /// Returns results sorted by relevance (highest score first)
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Record search request metric
        SEARCH_REQUESTS.inc();
        let start = Instant::now();

        info!(search_type = "vector", query = query, "Starting vector search");

        // Generate query embedding (use async version to avoid runtime nesting)
        let query_vector = self
            .embedder
            .embed_query_async(query)
            .await
            .with_context(|| format!("Failed to embed query: {}", query))?;

        debug!("Generated query embedding with {} dimensions", query_vector.len());

        // Perform vector search
        let mut results = self
            .storage
            .search(query_vector, limit)
            .await
            .with_context(|| "Failed to perform vector search")?;

        // Results are already sorted by score from LanceDB
        // But let's ensure they're sorted descending by score
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Record latency and result count metrics
        let elapsed = start.elapsed();
        SEARCH_LATENCY.observe(elapsed.as_secs_f64());
        SEARCH_RESULTS.observe(results.len() as f64);

        info!(
            search_type = "vector",
            query = query,
            results = results.len(),
            elapsed_ms = elapsed.as_millis() as u64,
            "Vector search completed"
        );

        Ok(results)
    }

    fn search_type(&self) -> &'static str {
        "vector"
    }
}
