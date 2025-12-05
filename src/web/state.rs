//! Application state for the web server.
//!
//! This module defines the shared state that is accessible from all request handlers.

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::search::traits::Search;
use crate::storage::Storage;

/// Shared application state for the web server.
///
/// This state is cloned for each request handler, but the inner Arc types
/// ensure that the actual data is shared efficiently.
#[derive(Clone)]
pub struct AppState {
    /// The search engine (vector, BM25, or hybrid)
    pub search_engine: Arc<dyn Search>,
    /// Storage backend for the vector database
    pub storage: Arc<Storage>,
    /// Embedding generator for query embeddings
    pub embedder: Arc<EmbeddingGenerator>,
    /// Configuration
    pub config: Config,
    /// Root path of the project
    pub root_path: PathBuf,
}

impl AppState {
    /// Create a new application state.
    pub fn new(
        search_engine: Arc<dyn Search>,
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        config: Config,
        root_path: PathBuf,
    ) -> Self {
        Self {
            search_engine,
            storage,
            embedder,
            config,
            root_path,
        }
    }
}
