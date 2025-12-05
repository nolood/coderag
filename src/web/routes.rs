//! Route definitions for the web server.
//!
//! This module defines all the HTTP routes for the CodeRAG web UI.

use axum::{
    routing::{get, post},
    Router,
};

use super::handlers;
use super::state::AppState;

/// Create the main router with all routes.
///
/// # Arguments
/// * `state` - The shared application state
///
/// # Returns
/// An Axum router configured with all CodeRAG web endpoints
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Main page
        .route("/", get(handlers::index_page))
        // API endpoints
        .route("/api/search", post(handlers::search))
        .route("/api/files", get(handlers::list_files))
        .route("/api/files/*path", get(handlers::get_file))
        .route("/api/stats", get(handlers::stats))
        .route("/api/reindex", post(handlers::reindex))
        // Health and metrics
        .route("/health", get(handlers::health))
        .route("/metrics", get(handlers::metrics_handler))
        // Static files fallback
        .fallback(get(handlers::static_file))
        .with_state(state)
}
