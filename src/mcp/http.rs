//! HTTP/SSE transport implementation for CodeRAG MCP server.
//!
//! This module provides HTTP transport using Server-Sent Events (SSE)
//! for the Model Context Protocol, enabling remote MCP clients to connect
//! to the CodeRAG server over HTTP.

use anyhow::Result;
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::search::SearchEngine;
use crate::storage::Storage;

use super::server::CodeRagServer;

/// Configuration for HTTP/SSE transport
#[derive(Debug, Clone)]
pub struct HttpTransportConfig {
    /// Address to bind the HTTP server to
    pub bind_addr: SocketAddr,
    /// Path for the SSE endpoint
    pub sse_path: String,
    /// Path for the POST message endpoint
    pub post_path: String,
}

impl Default for HttpTransportConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".parse().expect("valid default address"),
            sse_path: "/sse".to_string(),
            post_path: "/message".to_string(),
        }
    }
}

impl HttpTransportConfig {
    /// Create a new configuration with the specified port
    pub fn with_port(port: u16) -> Self {
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            ..Default::default()
        }
    }

    /// Create a new configuration with the specified host and port
    pub fn with_addr(host: [u8; 4], port: u16) -> Self {
        Self {
            bind_addr: SocketAddr::from((host, port)),
            ..Default::default()
        }
    }
}

/// HTTP/SSE transport for MCP server
pub struct HttpTransport {
    config: HttpTransportConfig,
    search_engine: Arc<SearchEngine>,
    storage: Arc<Storage>,
    root_path: PathBuf,
}

impl HttpTransport {
    /// Create a new HTTP transport
    pub fn new(
        config: HttpTransportConfig,
        search_engine: Arc<SearchEngine>,
        storage: Arc<Storage>,
        root_path: PathBuf,
    ) -> Self {
        Self {
            config,
            search_engine,
            storage,
            root_path,
        }
    }

    /// Start the HTTP/SSE server
    ///
    /// This will bind to the configured address and start accepting connections.
    /// The server runs until a shutdown signal is received.
    pub async fn run(self) -> Result<()> {
        let ct = CancellationToken::new();

        let sse_config = SseServerConfig {
            bind: self.config.bind_addr,
            sse_path: self.config.sse_path.clone(),
            post_path: self.config.post_path.clone(),
            ct: ct.clone(),
            sse_keep_alive: None,
        };

        info!(
            "Starting MCP HTTP/SSE server on {}",
            self.config.bind_addr
        );
        info!("  SSE endpoint: {}", self.config.sse_path);
        info!("  Message endpoint: {}", self.config.post_path);

        let (sse_server, router) = SseServer::new(sse_config);
        let listener = tokio::net::TcpListener::bind(sse_server.config.bind).await?;

        let server_ct = sse_server.config.ct.child_token();

        // Spawn the HTTP server
        let server = axum::serve(listener, router).with_graceful_shutdown(async move {
            server_ct.cancelled().await;
        });

        tokio::spawn(async move {
            if let Err(e) = server.await {
                error!(error = %e, "HTTP/SSE server shutdown with error");
            }
        });

        // Clone values for the closure
        let search_engine = self.search_engine.clone();
        let storage = self.storage.clone();
        let root_path = self.root_path.clone();

        // Register service factory with the SSE server
        let service_ct = sse_server.with_service(move || {
            CodeRagServer::new(search_engine.clone(), storage.clone(), root_path.clone())
        });

        info!("MCP HTTP/SSE server is ready and accepting connections");

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
        info!("Shutdown signal received, stopping server...");

        service_ct.cancel();

        Ok(())
    }
}

/// Run the MCP server with HTTP/SSE transport
///
/// This is a convenience function that sets up and runs the HTTP transport
/// with the provided components.
///
/// # Arguments
///
/// * `search_engine` - The search engine instance
/// * `storage` - The storage instance
/// * `root_path` - The project root path
/// * `port` - The port to bind to
pub async fn run_http_server(
    search_engine: Arc<SearchEngine>,
    storage: Arc<Storage>,
    root_path: PathBuf,
    port: u16,
) -> Result<()> {
    let config = HttpTransportConfig::with_port(port);
    let transport = HttpTransport::new(config, search_engine, storage, root_path);
    transport.run().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HttpTransportConfig::default();
        assert_eq!(config.bind_addr.port(), 3000);
        assert_eq!(config.sse_path, "/sse");
        assert_eq!(config.post_path, "/message");
    }

    #[test]
    fn test_config_with_port() {
        let config = HttpTransportConfig::with_port(8080);
        assert_eq!(config.bind_addr.port(), 8080);
        assert_eq!(config.bind_addr.ip().to_string(), "127.0.0.1");
    }

    #[test]
    fn test_config_with_addr() {
        let config = HttpTransportConfig::with_addr([0, 0, 0, 0], 9000);
        assert_eq!(config.bind_addr.port(), 9000);
        assert_eq!(config.bind_addr.ip().to_string(), "0.0.0.0");
    }
}
