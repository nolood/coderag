//! MCP server implementation for CodeRAG semantic code search.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

use crate::search::traits::Search;
use crate::search::SearchEngine;
use crate::storage::Storage;

/// Request parameters for semantic code search
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    /// Search query for semantic code search
    #[schemars(description = "Natural language query to search for relevant code")]
    query: String,

    /// Maximum number of results to return (default: 10)
    #[schemars(description = "Maximum number of results to return (default: 10)")]
    limit: Option<usize>,
}

/// Request parameters for listing indexed files
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListFilesRequest {
    /// Optional glob pattern to filter files (e.g., "*.rs", "src/**/*.ts")
    #[schemars(description = "Optional glob pattern to filter files (e.g., '*.rs', 'src/**/*.ts')")]
    pattern: Option<String>,
}

/// Request parameters for retrieving file content
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFileRequest {
    /// Path to the file relative to project root
    #[schemars(description = "Path to the file relative to the project root")]
    path: String,
}

/// MCP server for CodeRAG semantic code search
///
/// This server provides semantic code search capabilities over the Model Context Protocol.
/// It can be used with both stdio and HTTP/SSE transports.
///
/// # Example
///
/// ```ignore
/// use std::sync::Arc;
/// use coderag::mcp::CodeRagServer;
///
/// let server = CodeRagServer::new(search_engine, storage, root_path);
///
/// // Run with stdio transport
/// server.run().await?;
/// ```
#[derive(Clone)]
pub struct CodeRagServer {
    search_engine: Arc<SearchEngine>,
    storage: Arc<Storage>,
    root_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CodeRagServer {
    /// Create a new CodeRAG MCP server
    pub fn new(
        search_engine: Arc<SearchEngine>,
        storage: Arc<Storage>,
        root_path: PathBuf,
    ) -> Self {
        Self {
            search_engine,
            storage,
            root_path,
            tool_router: Self::tool_router(),
        }
    }

    /// Search for relevant code snippets using semantic search
    #[tool(
        name = "search",
        description = "Search for relevant code snippets using semantic search. Returns matching code chunks with file paths, line numbers, content, and relevance scores."
    )]
    async fn search(
        &self,
        Parameters(req): Parameters<SearchRequest>,
    ) -> Result<CallToolResult, McpError> {
        let limit = req.limit.unwrap_or(10);

        let results = self
            .search_engine
            .search(&req.query, limit)
            .await
            .map_err(|e| McpError::internal_error(format!("Search failed: {}", e), None))?;

        // Format results as readable text
        let mut output = String::new();

        if results.is_empty() {
            output.push_str("No results found for the query.");
        } else {
            output.push_str(&format!(
                "Found {} result(s) for query: \"{}\"\n\n",
                results.len(),
                req.query
            ));

            for (i, result) in results.iter().enumerate() {
                output.push_str(&format!(
                    "## Result {} (relevance: {:.1}%)\n",
                    i + 1,
                    result.score * 100.0
                ));
                output.push_str(&format!(
                    "**File:** {}:{}-{}\n",
                    result.file_path, result.start_line, result.end_line
                ));
                output.push_str("```\n");
                output.push_str(&result.content);
                if !result.content.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str("```\n\n");
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// List indexed files with optional glob pattern filter
    #[tool(
        name = "list_files",
        description = "List all indexed files in the codebase, optionally filtered by a glob pattern (e.g., '*.rs', 'src/**/*.ts')."
    )]
    async fn list_files(
        &self,
        Parameters(req): Parameters<ListFilesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let files = self
            .storage
            .list_files(req.pattern.as_deref())
            .await
            .map_err(|e| McpError::internal_error(format!("Failed to list files: {}", e), None))?;

        let output = if files.is_empty() {
            match &req.pattern {
                Some(p) => format!("No indexed files matching pattern: {}", p),
                None => "No files have been indexed yet.".to_string(),
            }
        } else {
            let header = match &req.pattern {
                Some(p) => format!("Indexed files matching '{}' ({} files):\n", p, files.len()),
                None => format!("All indexed files ({} files):\n", files.len()),
            };
            format!("{}{}", header, files.join("\n"))
        };

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Get the full content of a file
    #[tool(
        name = "get_file",
        description = "Get the full content of a file by its path relative to the project root."
    )]
    async fn get_file(
        &self,
        Parameters(req): Parameters<GetFileRequest>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = self.root_path.join(&req.path);

        // Canonicalize paths for security check
        let canonical = file_path.canonicalize().map_err(|e| {
            McpError::invalid_params(format!("Invalid path '{}': {}", req.path, e), None)
        })?;

        let root_canonical = self.root_path.canonicalize().map_err(|e| {
            McpError::internal_error(format!("Failed to resolve root path: {}", e), None)
        })?;

        // Security check: ensure the resolved path is within the project root
        if !canonical.starts_with(&root_canonical) {
            return Err(McpError::invalid_params(
                format!(
                    "Path '{}' is outside the project root. Access denied.",
                    req.path
                ),
                None,
            ));
        }

        // Read the file content
        let content = tokio::fs::read_to_string(&canonical).await.map_err(|e| {
            McpError::invalid_params(format!("Failed to read file '{}': {}", req.path, e), None)
        })?;

        // Format output with file information
        let output = format!(
            "# File: {}\n\n```\n{}\n```",
            req.path,
            content.trim_end()
        );

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Run the MCP server using stdio transport
    pub async fn run(self) -> anyhow::Result<()> {
        let service = self.serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    }

    /// Get a reference to the search engine
    pub fn search_engine(&self) -> &Arc<SearchEngine> {
        &self.search_engine
    }

    /// Get a reference to the storage
    pub fn storage(&self) -> &Arc<Storage> {
        &self.storage
    }

    /// Get a reference to the root path
    pub fn root_path(&self) -> &PathBuf {
        &self.root_path
    }
}

#[tool_handler]
impl ServerHandler for CodeRagServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "coderag".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("CodeRAG Semantic Search".into()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "CodeRAG MCP Server - Semantic code search for your codebase.\n\n\
                 Available tools:\n\
                 - search: Find relevant code using natural language queries\n\
                 - list_files: View all indexed files with optional glob filtering\n\
                 - get_file: Read the full content of any indexed file\n\n\
                 Use 'search' to discover code patterns, implementations, and examples. \
                 Use 'list_files' to explore the codebase structure. \
                 Use 'get_file' to examine specific files in detail."
                    .into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_request_default_limit() {
        let json = r#"{"query": "error handling"}"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "error handling");
        assert!(req.limit.is_none());
    }

    #[test]
    fn test_search_request_with_limit() {
        let json = r#"{"query": "async function", "limit": 5}"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.query, "async function");
        assert_eq!(req.limit, Some(5));
    }

    #[test]
    fn test_list_files_request_no_pattern() {
        let json = r#"{}"#;
        let req: ListFilesRequest = serde_json::from_str(json).unwrap();
        assert!(req.pattern.is_none());
    }

    #[test]
    fn test_list_files_request_with_pattern() {
        let json = r#"{"pattern": "*.rs"}"#;
        let req: ListFilesRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.pattern, Some("*.rs".to_string()));
    }

    #[test]
    fn test_get_file_request() {
        let json = r#"{"path": "src/main.rs"}"#;
        let req: GetFileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.path, "src/main.rs");
    }
}
