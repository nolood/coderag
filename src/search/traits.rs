//! Search trait for polymorphic search implementations.
//!
//! This module defines the common `Search` trait that all search implementations
//! (vector, BM25, hybrid) must implement.

use anyhow::Result;
use async_trait::async_trait;

use crate::storage::SearchResult;

/// Common trait for all search implementations.
///
/// This trait provides a unified interface for different search strategies:
/// - Vector search (semantic similarity)
/// - BM25 search (keyword matching)
/// - Hybrid search (combining both)
#[async_trait]
pub trait Search: Send + Sync {
    /// Search for relevant code chunks.
    ///
    /// # Arguments
    /// * `query` - The search query string
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    /// A vector of search results sorted by relevance (highest score first)
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Get the search type identifier.
    ///
    /// Returns a string identifying the search implementation type,
    /// such as "vector", "bm25", or "hybrid".
    fn search_type(&self) -> &'static str;
}

/// A scored result with its rank position.
///
/// Used internally for fusion algorithms.
#[derive(Debug, Clone)]
pub struct RankedResult {
    /// The search result
    pub result: SearchResult,
    /// The rank position (1-indexed)
    pub rank: usize,
    /// The original score from the search implementation
    pub score: f32,
}

impl RankedResult {
    /// Create a new ranked result.
    pub fn new(result: SearchResult, rank: usize) -> Self {
        let score = result.score;
        Self {
            result,
            rank,
            score,
        }
    }
}
