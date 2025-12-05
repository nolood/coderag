//! Web UI module for CodeRAG debugging interface.
//!
//! This module provides a simple web-based UI for:
//! - Searching the codebase
//! - Viewing index statistics
//! - Browsing indexed files
//! - Monitoring metrics
//!
//! # Example
//!
//! ```rust,ignore
//! use coderag::web::WebServer;
//!
//! let server = WebServer::new(state);
//! server.start(8080).await?;
//! ```

pub mod handlers;
pub mod routes;
pub mod state;

pub use state::AppState;

use anyhow::{Context, Result};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

/// Web server for the CodeRAG debug UI.
///
/// Provides a simple HTTP server with endpoints for searching,
/// viewing statistics, and browsing indexed files.
pub struct WebServer {
    /// Shared application state
    state: AppState,
}

impl WebServer {
    /// Create a new web server with the given state.
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Start the web server on the specified port.
    ///
    /// This method blocks until the server is shut down.
    ///
    /// # Arguments
    /// * `port` - The port to listen on
    ///
    /// # Returns
    /// An error if the server fails to start
    pub async fn start(self, port: u16) -> Result<()> {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        // Configure CORS for local development
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // Create the router with CORS middleware
        let app = routes::create_router(self.state).layer(cors);

        info!("Starting web server at http://{}", addr);
        info!("Open http://localhost:{} in your browser", port);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("Failed to bind to port {}", port))?;

        axum::serve(listener, app)
            .await
            .with_context(|| "Web server failed")?;

        Ok(())
    }
}
