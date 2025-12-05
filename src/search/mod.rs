//! Search module providing vector, BM25, and hybrid search capabilities.
//!
//! This module contains:
//! - `traits` - Common `Search` trait for all search implementations
//! - `vector` - Semantic vector search using embeddings
//! - `bm25` - BM25 keyword search using Tantivy
//! - `hybrid` - Hybrid search combining vector and BM25 with RRF fusion

pub mod bm25;
pub mod hybrid;
pub mod traits;
mod vector;

// Re-export commonly used types
pub use bm25::{Bm25Index, Bm25Search};
pub use hybrid::{HybridSearch, RrfFusion};
pub use traits::Search;
pub use vector::{SearchEngine, SearchResult};
