//! MCP server command implementation.
//!
//! Starts the CodeRAG MCP server for integration with LLM clients.
//! Supports both stdio and HTTP/SSE transports.
//!
//! With zero-ceremony mode, the server can auto-detect the project
//! and auto-index on startup if needed.

use anyhow::Result;
use std::env;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{debug, info};

use crate::auto_index::{AutoIndexPolicy, AutoIndexService};
use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::mcp::{run_http_server, CodeRagServer, Transport};
use crate::search::SearchEngine;
use crate::storage::Storage;
use crate::symbol::SymbolIndex;
use crate::watcher::{FileWatcher, ProcessingStats, WatcherConfig};

/// Default port for HTTP transport
const DEFAULT_HTTP_PORT: u16 = 3000;

/// Run the MCP server command
///
/// Initializes all required components (storage, embedder, search engine)
/// and starts the MCP server using the specified transport.
///
/// With zero-ceremony support, this command:
/// 1. Auto-detects the project root
/// 2. Resolves storage location (local or global)
/// 3. Auto-indexes if needed (unless `no_auto_index` is set)
/// 4. Optionally starts a file watcher in parallel
/// 5. Starts the MCP server
///
/// # Arguments
///
/// * `http` - Use HTTP/SSE transport instead of stdio
/// * `port` - Port for HTTP transport (default: 3000)
/// * `no_auto_index` - Skip auto-indexing on startup
/// * `watch` - Start file watcher in parallel with MCP server
/// * `debounce_ms` - Debounce delay in milliseconds for the file watcher
pub async fn run(
    http: bool,
    port: Option<u16>,
    no_auto_index: bool,
    watch: bool,
    debounce_ms: u64,
) -> Result<()> {
    let cwd = env::current_dir()?;

    // Set up auto-index service with appropriate policy
    let policy = if no_auto_index {
        AutoIndexPolicy::Never
    } else {
        AutoIndexPolicy::OnMissing
    };
    let service = AutoIndexService::with_policy(policy);
    let result = service.ensure_indexed(&cwd).await?;

    // Show indexing message if first time
    if result.files_indexed > 0 {
        eprintln!(
            "Indexed {} files ({} chunks) for MCP server",
            result.files_indexed, result.chunks_created
        );
    }

    // Load config from resolved storage location
    let config = if result.storage.is_local() {
        Config::load(result.storage.root())?
    } else {
        Config::default()
    };

    // Determine transport type based on --http flag
    let transport_type = if http {
        Transport::Http
    } else {
        Transport::Stdio
    };

    // Initialize embedding generator first (needed for vector dimension)
    let embedder = Arc::new(
        EmbeddingGenerator::new_async(&config.embeddings)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize embeddings: {}", e))?,
    );

    // Initialize storage using resolved path and embedding dimension
    let vector_dimension = embedder.embedding_dimension();
    let storage = Arc::new(
        Storage::new(result.storage.db_path(), vector_dimension)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize storage: {}", e))?,
    );

    // Initialize search engine
    let search_engine = Arc::new(SearchEngine::new(storage.clone(), embedder.clone()));

    // Build symbol index from stored chunks
    info!("Building symbol index from stored chunks...");
    let chunks = storage.get_all_chunks().await?;
    let symbol_index = Arc::new(SymbolIndex::build_from_chunks(&chunks));
    info!("Symbol index ready with {} symbols", symbol_index.symbol_count());

    // Use project root from storage resolution
    let project_root = result.storage.root().to_path_buf();

    // Set up file watcher if requested
    let watcher_handle: Option<(oneshot::Sender<()>, JoinHandle<Result<ProcessingStats>>)> =
        if watch {
            let watcher_config = WatcherConfig::from_config(&config, debounce_ms);
            let watcher = FileWatcher::new(
                project_root.clone(),
                watcher_config,
                storage.clone(),
                embedder.clone(),
                config.clone(),
            );

            let (shutdown_tx, shutdown_rx) = oneshot::channel();

            eprintln!("Starting file watcher in background...");
            eprintln!("  Debounce delay: {}ms", debounce_ms);
            eprintln!("  Extensions: {:?}", config.indexer.extensions);

            let handle = tokio::spawn(async move { watcher.run(shutdown_rx).await });

            Some((shutdown_tx, handle))
        } else {
            None
        };

    // Start server with the appropriate transport
    match transport_type {
        Transport::Stdio => {
            info!("Starting MCP server with stdio transport");
            let server = CodeRagServer::new(search_engine, storage, symbol_index, project_root);
            server.run().await?;
        }
        Transport::Http => {
            let port = port.unwrap_or(DEFAULT_HTTP_PORT);
            info!("Starting MCP server with HTTP/SSE transport on port {}", port);
            run_http_server(search_engine, storage, symbol_index, project_root, port).await?;
        }
    }

    // Shutdown watcher and print statistics
    if let Some((shutdown_tx, handle)) = watcher_handle {
        eprintln!();
        eprintln!("Shutting down file watcher...");

        // Send shutdown signal
        // Note: Error means the receiver was already dropped (watcher finished early)
        if shutdown_tx.send(()).is_err() {
            debug!("Watcher shutdown channel already closed (watcher may have finished early)");
        }

        // Wait for watcher to finish and get stats
        match handle.await {
            Ok(Ok(stats)) => {
                print_watcher_stats(&stats);
            }
            Ok(Err(e)) => {
                eprintln!("File watcher error: {}", e);
            }
            Err(e) => {
                eprintln!("File watcher task panicked: {}", e);
            }
        }
    }

    Ok(())
}

/// Print file watcher statistics
fn print_watcher_stats(stats: &ProcessingStats) {
    eprintln!();
    eprintln!("File watcher session complete!");
    eprintln!("----------------------------------------");
    eprintln!("  Files added:    {}", stats.files_added);
    eprintln!("  Files modified: {}", stats.files_modified);
    eprintln!("  Files deleted:  {}", stats.files_deleted);
    eprintln!("  Chunks created: {}", stats.chunks_created);
    eprintln!("  Chunks removed: {}", stats.chunks_removed);
    if stats.errors > 0 {
        eprintln!("  Errors:         {}", stats.errors);
    }
    eprintln!("----------------------------------------");
}
