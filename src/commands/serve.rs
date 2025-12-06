//! MCP server command implementation.
//!
//! Starts the CodeRAG MCP server for integration with LLM clients.
//! Supports both stdio and HTTP/SSE transports.

use anyhow::{bail, Result};
use std::env;
use std::sync::Arc;
use tracing::info;

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::mcp::{run_http_server, CodeRagServer, Transport};
use crate::search::SearchEngine;
use crate::storage::Storage;
use crate::symbol::SymbolIndex;

/// Default port for HTTP transport
const DEFAULT_HTTP_PORT: u16 = 3000;

/// Run the MCP server command
///
/// Initializes all required components (storage, embedder, search engine)
/// and starts the MCP server using the specified transport.
///
/// # Arguments
///
/// * `transport` - Transport type ("stdio" or "http")
/// * `port` - Port for HTTP transport (default: 3000)
pub async fn run(transport: &str, port: Option<u16>) -> Result<()> {
    let root = env::current_dir()?;

    // Verify CodeRAG is initialized
    if !Config::is_initialized(&root) {
        bail!(
            "CodeRAG is not initialized in this directory.\n\
             Run 'coderag init' first to initialize the project."
        );
    }

    let config = Config::load(&root)?;

    // Parse transport type
    let transport_type = Transport::parse(transport).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown transport type: '{}'. Valid options are: stdio, http, sse",
            transport
        )
    })?;

    // Initialize storage
    let db_path = config.db_path(&root);
    let storage = Arc::new(
        Storage::new(&db_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize storage: {}", e))?,
    );

    // Initialize embedding generator
    let embedder = Arc::new(
        EmbeddingGenerator::new(&config.embeddings)
            .map_err(|e| anyhow::anyhow!("Failed to initialize embeddings: {}", e))?,
    );

    // Initialize search engine
    let search_engine = Arc::new(SearchEngine::new(storage.clone(), embedder));

    // Build symbol index from stored chunks
    info!("Building symbol index from stored chunks...");
    let chunks = storage.get_all_chunks().await?;
    let symbol_index = Arc::new(SymbolIndex::build_from_chunks(&chunks));
    info!("Symbol index ready with {} symbols", symbol_index.symbol_count());

    // Start server with the appropriate transport
    match transport_type {
        Transport::Stdio => {
            info!("Starting MCP server with stdio transport");
            let server = CodeRagServer::new(search_engine, storage, symbol_index, root);
            server.run().await?;
        }
        Transport::Http => {
            let port = port.unwrap_or(DEFAULT_HTTP_PORT);
            info!("Starting MCP server with HTTP/SSE transport on port {}", port);
            run_http_server(search_engine, storage, symbol_index, root, port).await?;
        }
    }

    Ok(())
}
