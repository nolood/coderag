//! HTTP request handlers for the web UI.
//!
//! This module contains all the API endpoint handlers for the CodeRAG web interface.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    Json,
};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{error, info};

use super::state::AppState;
use crate::config::SearchMode;
use crate::metrics;

/// Embedded static files for the web UI.
#[derive(Embed)]
#[folder = "src/web/static/"]
struct StaticAssets;

/// Search request payload.
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    /// The search query string
    pub query: String,
    /// Maximum number of results to return
    pub limit: Option<usize>,
    /// Search mode: "vector", "bm25", or "hybrid"
    pub mode: Option<SearchMode>,
}

/// Search response payload.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    /// Search results
    pub results: Vec<SearchResultDto>,
    /// The original query
    pub query: String,
    /// Search mode used
    pub mode: String,
    /// Time taken in milliseconds
    pub took_ms: u64,
}

/// A single search result for the API.
#[derive(Debug, Serialize)]
pub struct SearchResultDto {
    /// File path relative to project root
    pub file_path: String,
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Ending line number (1-indexed)
    pub end_line: usize,
    /// The content of the chunk
    pub content: String,
    /// Relevance score
    pub score: f32,
}

/// Statistics response payload.
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    /// Total number of indexed files
    pub files: usize,
    /// Total number of chunks in the index
    pub chunks: usize,
    /// Index storage size in bytes
    pub index_size: u64,
    /// Current search mode
    pub search_mode: String,
    /// Project root path
    pub root_path: String,
}

/// File info for the file browser.
#[derive(Debug, Serialize)]
pub struct FileInfo {
    /// File path relative to project root
    pub path: String,
    /// File extension
    pub extension: Option<String>,
}

/// Reindex response payload.
#[derive(Debug, Serialize)]
pub struct ReindexResponse {
    /// Whether reindex was triggered successfully
    pub success: bool,
    /// Status message
    pub message: String,
}

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Health status
    pub status: String,
    /// Version
    pub version: String,
}

/// Serve the main index page.
pub async fn index_page() -> impl IntoResponse {
    match StaticAssets::get("index.html") {
        Some(content) => Html(content.data.to_vec()).into_response(),
        None => (StatusCode::NOT_FOUND, "Index page not found").into_response(),
    }
}

/// Handle search requests.
///
/// POST /api/search
pub async fn search(
    State(state): State<AppState>,
    Json(request): Json<SearchRequest>,
) -> impl IntoResponse {
    let start = Instant::now();
    let limit = request.limit.unwrap_or(state.config.search.default_limit);

    info!(
        query = %request.query,
        limit = limit,
        "Processing search request"
    );

    match state.search_engine.search(&request.query, limit).await {
        Ok(results) => {
            let took_ms = start.elapsed().as_millis() as u64;

            let response = SearchResponse {
                results: results
                    .into_iter()
                    .map(|r| SearchResultDto {
                        file_path: r.file_path,
                        start_line: r.start_line,
                        end_line: r.end_line,
                        content: r.content,
                        score: r.score,
                    })
                    .collect(),
                query: request.query,
                mode: state.search_engine.search_type().to_string(),
                took_ms,
            };

            info!(
                results = response.results.len(),
                took_ms = took_ms,
                "Search completed"
            );

            Json(response).into_response()
        }
        Err(e) => {
            error!(error = %e, "Search failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Search failed: {}", e)
                })),
            )
                .into_response()
        }
    }
}

/// List indexed files.
///
/// GET /api/files
pub async fn list_files(State(state): State<AppState>) -> impl IntoResponse {
    match state.storage.list_files(None).await {
        Ok(files) => {
            let file_infos: Vec<FileInfo> = files
                .into_iter()
                .map(|path| {
                    let extension = std::path::Path::new(&path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_string());
                    FileInfo { path, extension }
                })
                .collect();

            Json(file_infos).into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to list files");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to list files: {}", e)
                })),
            )
                .into_response()
        }
    }
}

/// Get file content.
///
/// GET /api/files/*path
pub async fn get_file(
    State(state): State<AppState>,
    Path(file_path): Path<String>,
) -> impl IntoResponse {
    let full_path = state.root_path.join(&file_path);

    // Security: Canonicalize paths and verify the file is within project root
    let canonical = match full_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            error!(path = %file_path, error = %e, "Failed to resolve file path");
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("File not found: {}", file_path)
                })),
            )
                .into_response();
        }
    };

    let root_canonical = match state.root_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "Failed to resolve root path");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Internal server error"
                })),
            )
                .into_response();
        }
    };

    // Security check: ensure the resolved path is within the project root
    if !canonical.starts_with(&root_canonical) {
        error!(path = %file_path, "Path traversal attempt detected");
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "Access denied: path is outside project root"
            })),
        )
            .into_response();
    }

    match std::fs::read_to_string(&canonical) {
        Ok(content) => {
            let extension = std::path::Path::new(&file_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("txt");

            let content_type = mime_guess::from_ext(extension)
                .first_or_text_plain()
                .to_string();

            ([(header::CONTENT_TYPE, content_type)], content).into_response()
        }
        Err(e) => {
            error!(path = %file_path, error = %e, "Failed to read file");
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("File not found: {}", file_path)
                })),
            )
                .into_response()
        }
    }
}

/// Get index statistics.
///
/// GET /api/stats
pub async fn stats(State(state): State<AppState>) -> impl IntoResponse {
    let chunks = match state.storage.count_chunks().await {
        Ok(count) => count,
        Err(e) => {
            error!(error = %e, "Failed to count chunks");
            0
        }
    };

    let files = match state.storage.list_files(None).await {
        Ok(files) => files.len(),
        Err(e) => {
            error!(error = %e, "Failed to count files");
            0
        }
    };

    // Estimate index size from the database path
    let index_size = std::fs::metadata(state.storage.path())
        .map(|m| m.len())
        .unwrap_or(0);

    let response = StatsResponse {
        files,
        chunks,
        index_size,
        search_mode: state.search_engine.search_type().to_string(),
        root_path: state.root_path.to_string_lossy().to_string(),
    };

    Json(response)
}

/// Trigger a full reindex.
///
/// POST /api/reindex
pub async fn reindex(State(_state): State<AppState>) -> impl IntoResponse {
    // For now, just return a message indicating the user should run `coderag index`
    // A full async reindex would require more infrastructure
    let response = ReindexResponse {
        success: false,
        message: "Please run `coderag index` from the command line to reindex the codebase."
            .to_string(),
    };

    Json(response)
}

/// Health check endpoint.
///
/// GET /health
pub async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Prometheus metrics endpoint.
///
/// GET /metrics
pub async fn metrics_handler() -> impl IntoResponse {
    let output = metrics::gather_metrics();
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], output)
}

/// Serve static files.
///
/// Fallback handler for static assets.
pub async fn static_file(Path(path): Path<String>) -> impl IntoResponse {
    // Try to serve the requested file
    let path = path.trim_start_matches('/');

    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => {
            // If not found, return 404
            (StatusCode::NOT_FOUND, "File not found").into_response()
        }
    }
}
