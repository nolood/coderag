//! Hybrid search combining vector similarity and BM25 keyword matching.
//!
//! This module implements hybrid search using Reciprocal Rank Fusion (RRF)
//! to combine results from vector search and BM25 search.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

use super::bm25::Bm25Search;
use super::traits::Search;
use super::SearchEngine;
use crate::embeddings::EmbeddingGenerator;
use crate::storage::{SearchResult, Storage};

/// Default RRF constant (k parameter).
///
/// Higher values make the ranking smoother, lower values emphasize top ranks more.
const DEFAULT_RRF_K: f32 = 60.0;

/// Default weight for vector search results.
const DEFAULT_VECTOR_WEIGHT: f32 = 0.7;

/// Default weight for BM25 search results.
const DEFAULT_BM25_WEIGHT: f32 = 0.3;

/// Reciprocal Rank Fusion (RRF) algorithm.
///
/// Combines multiple ranked result lists using the formula:
/// `score = sum(weight / (k + rank))` for each result across all lists.
pub struct RrfFusion {
    /// The k constant in the RRF formula
    k: f32,
}

impl RrfFusion {
    /// Create RRF with default k=60.
    pub fn new() -> Self {
        Self { k: DEFAULT_RRF_K }
    }

    /// Create RRF with custom k value.
    pub fn with_k(k: f32) -> Self {
        Self { k }
    }

    /// Fuse multiple ranked result lists into one.
    ///
    /// # Arguments
    /// * `results` - Vector of (result_list, weight) pairs
    /// * `limit` - Maximum results to return
    ///
    /// # Returns
    /// A fused list of search results sorted by combined RRF score
    pub fn fuse(&self, results: Vec<(Vec<SearchResult>, f32)>, limit: usize) -> Vec<SearchResult> {
        // Map from unique identifier to (SearchResult, combined RRF score)
        let mut fused_scores: HashMap<String, (SearchResult, f32)> = HashMap::new();

        for (result_list, weight) in results {
            for (rank, result) in result_list.into_iter().enumerate() {
                // Create unique key from file_path and line range
                let key = format!("{}:{}:{}", result.file_path, result.start_line, result.end_line);

                // RRF score: weight / (k + rank)
                // rank is 0-indexed, but RRF traditionally uses 1-indexed ranks
                let rrf_score = weight / (self.k + (rank + 1) as f32);

                fused_scores
                    .entry(key)
                    .and_modify(|(_, score)| *score += rrf_score)
                    .or_insert((result, rrf_score));
            }
        }

        // Sort by fused score descending
        let mut sorted: Vec<_> = fused_scores.into_values().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results and update scores
        sorted
            .into_iter()
            .take(limit)
            .map(|(mut result, score)| {
                result.score = score;
                result
            })
            .collect()
    }
}

impl Default for RrfFusion {
    fn default() -> Self {
        Self::new()
    }
}

/// Hybrid search combining vector similarity and BM25 keyword matching.
///
/// Uses Reciprocal Rank Fusion (RRF) to combine results from both search methods,
/// allowing for configurable weights between semantic and keyword search.
pub struct HybridSearch {
    /// Vector search engine
    vector: SearchEngine,
    /// BM25 search engine
    bm25: Bm25Search,
    /// RRF fusion algorithm
    fusion: RrfFusion,
    /// Weight for vector results (0.0 - 1.0)
    vector_weight: f32,
    /// Weight for BM25 results (0.0 - 1.0)
    bm25_weight: f32,
}

impl HybridSearch {
    /// Create a new hybrid search engine.
    ///
    /// # Arguments
    /// * `storage` - Vector storage
    /// * `embedder` - Embedding generator
    /// * `bm25_path` - Path to .coderag directory for BM25 index
    /// * `vector_weight` - Weight for vector results (default 0.7)
    /// * `bm25_weight` - Weight for BM25 results (default 0.3)
    pub fn new(
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        bm25_path: &Path,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> Result<Self> {
        let vector = SearchEngine::new(storage, embedder);
        let bm25 = Bm25Search::new(bm25_path)
            .with_context(|| "Failed to create BM25 search engine")?;

        Ok(Self {
            vector,
            bm25,
            fusion: RrfFusion::new(),
            vector_weight,
            bm25_weight,
        })
    }

    /// Create a new hybrid search engine with default weights (0.7 vector, 0.3 BM25).
    pub fn with_defaults(
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        bm25_path: &Path,
    ) -> Result<Self> {
        Self::new(
            storage,
            embedder,
            bm25_path,
            DEFAULT_VECTOR_WEIGHT,
            DEFAULT_BM25_WEIGHT,
        )
    }

    /// Set a custom RRF fusion algorithm.
    pub fn with_fusion(mut self, fusion: RrfFusion) -> Self {
        self.fusion = fusion;
        self
    }

    /// Set custom RRF k value.
    pub fn with_rrf_k(mut self, k: f32) -> Self {
        self.fusion = RrfFusion::with_k(k);
        self
    }

    /// Get access to the BM25 search engine.
    pub fn bm25(&self) -> &Bm25Search {
        &self.bm25
    }

    /// Get access to the vector search engine.
    pub fn vector(&self) -> &SearchEngine {
        &self.vector
    }

    /// Get the configured weights.
    pub fn weights(&self) -> (f32, f32) {
        (self.vector_weight, self.bm25_weight)
    }
}

#[async_trait]
impl Search for HybridSearch {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let start = std::time::Instant::now();

        // Fetch more results from each search to ensure good fusion
        let fetch_limit = limit * 3;

        // Run both searches concurrently
        let (vector_results, bm25_results) = tokio::join!(
            self.vector.search(query, fetch_limit),
            self.bm25.search(query, fetch_limit)
        );

        let vector_results = vector_results.with_context(|| "Vector search failed")?;
        let bm25_results = bm25_results.with_context(|| "BM25 search failed")?;

        // Fuse results using RRF
        let fused = self.fusion.fuse(
            vec![
                (vector_results, self.vector_weight),
                (bm25_results, self.bm25_weight),
            ],
            limit,
        );

        let elapsed = start.elapsed();
        info!(
            search_type = "hybrid",
            query = query,
            results = fused.len(),
            vector_weight = self.vector_weight,
            bm25_weight = self.bm25_weight,
            elapsed_ms = elapsed.as_millis() as u64,
            "Hybrid search completed"
        );

        Ok(fused)
    }

    fn search_type(&self) -> &'static str {
        "hybrid"
    }
}

/// Compute RRF score for a single result across multiple rankings.
///
/// # Arguments
/// * `ranks` - Vector of (rank, weight) pairs where rank is 1-indexed
/// * `k` - RRF constant (typically 60)
///
/// # Returns
/// The combined RRF score
pub fn rrf_score(ranks: &[(usize, f32)], k: f32) -> f32 {
    ranks
        .iter()
        .map(|(rank, weight)| weight / (k + *rank as f32))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_result(file_path: &str, start_line: usize, score: f32) -> SearchResult {
        SearchResult {
            content: format!("content at line {}", start_line),
            file_path: file_path.to_string(),
            start_line,
            end_line: start_line + 10,
            score,
            file_header: None,
        }
    }

    #[test]
    fn test_rrf_score() {
        // Single ranking with weight 1.0 at rank 1
        let score = rrf_score(&[(1, 1.0)], 60.0);
        assert!((score - 1.0 / 61.0).abs() < 0.0001);

        // Two rankings
        let score = rrf_score(&[(1, 0.7), (2, 0.3)], 60.0);
        let expected = 0.7 / 61.0 + 0.3 / 62.0;
        assert!((score - expected).abs() < 0.0001);
    }

    #[test]
    fn test_rrf_fusion_basic() {
        let fusion = RrfFusion::new();

        let vector_results = vec![
            create_test_result("file1.rs", 1, 0.9),
            create_test_result("file2.rs", 1, 0.8),
        ];

        let bm25_results = vec![
            create_test_result("file2.rs", 1, 0.95),
            create_test_result("file3.rs", 1, 0.85),
        ];

        let fused = fusion.fuse(
            vec![(vector_results, 0.7), (bm25_results, 0.3)],
            10,
        );

        // file2.rs should be ranked highest as it appears in both
        assert_eq!(fused.len(), 3);
        assert_eq!(fused[0].file_path, "file2.rs");
    }

    #[test]
    fn test_rrf_fusion_limit() {
        let fusion = RrfFusion::new();

        let results1 = vec![
            create_test_result("file1.rs", 1, 0.9),
            create_test_result("file2.rs", 1, 0.8),
            create_test_result("file3.rs", 1, 0.7),
        ];

        let results2 = vec![
            create_test_result("file4.rs", 1, 0.95),
            create_test_result("file5.rs", 1, 0.85),
        ];

        let fused = fusion.fuse(vec![(results1, 0.7), (results2, 0.3)], 2);

        assert_eq!(fused.len(), 2);
    }

    #[test]
    fn test_rrf_custom_k() {
        let fusion_low_k = RrfFusion::with_k(10.0);
        let fusion_high_k = RrfFusion::with_k(100.0);

        let results = vec![
            create_test_result("file1.rs", 1, 0.9),
            create_test_result("file2.rs", 1, 0.8),
        ];

        let fused_low = fusion_low_k.fuse(vec![(results.clone(), 1.0)], 10);
        let fused_high = fusion_high_k.fuse(vec![(results, 1.0)], 10);

        // With lower k, the score difference between ranks is larger
        let diff_low = fused_low[0].score - fused_low[1].score;
        let diff_high = fused_high[0].score - fused_high[1].score;

        assert!(diff_low > diff_high);
    }
}
