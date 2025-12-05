//! Web UI command implementation.
//!
//! This module provides the `coderag web` command which starts a local
//! web server for debugging and exploring the indexed codebase.

use anyhow::{bail, Result};
use std::env;
use std::sync::Arc;

use crate::config::SearchMode;
use crate::embeddings::EmbeddingGenerator;
use crate::search::{HybridSearch, SearchEngine};
use crate::storage::Storage;
use crate::web::{AppState, WebServer};
use crate::Config;

/// Run the web server command.
///
/// # Arguments
/// * `port` - The port to listen on (default: 8080)
pub async fn run(port: u16) -> Result<()> {
    let root = env::current_dir()?;

    if !Config::is_initialized(&root) {
        bail!("CodeRAG is not initialized. Run 'coderag init' first.");
    }

    let config = Config::load(&root)?;

    // Check if there's any indexed data
    let storage = Arc::new(Storage::new(&config.db_path(&root)).await?);
    let chunk_count = storage.count_chunks().await?;

    if chunk_count == 0 {
        println!("Warning: No indexed data found. Run 'coderag index' first.");
        println!("The web UI will still start, but search will return no results.\n");
    }

    // Initialize the embedding generator
    let embedder = Arc::new(EmbeddingGenerator::new(&config.embeddings)?);

    // Create the search engine based on configured mode
    let search_engine: Arc<dyn crate::search::traits::Search> = match config.search.mode {
        SearchMode::Vector => {
            Arc::new(SearchEngine::new(Arc::clone(&storage), Arc::clone(&embedder)))
        }
        SearchMode::Hybrid | SearchMode::Bm25 => {
            // For hybrid or BM25 mode, use HybridSearch
            let coderag_dir = Config::coderag_dir(&root);
            match HybridSearch::new(
                Arc::clone(&storage),
                Arc::clone(&embedder),
                &coderag_dir,
                config.search.vector_weight,
                config.search.bm25_weight,
            ) {
                Ok(hybrid) => Arc::new(hybrid.with_rrf_k(config.search.rrf_k)),
                Err(e) => {
                    // Fall back to vector search if hybrid fails
                    tracing::warn!(
                        "Failed to initialize hybrid search, falling back to vector: {}",
                        e
                    );
                    Arc::new(SearchEngine::new(Arc::clone(&storage), Arc::clone(&embedder)))
                }
            }
        }
    };

    // Create the application state
    let state = AppState::new(
        search_engine,
        storage,
        embedder,
        config,
        root,
    );

    // Start the web server
    let server = WebServer::new(state);
    server.start(port).await
}
