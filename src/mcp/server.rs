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
use crate::symbol::{
    FindReferencesRequest, FindSymbolRequest, ListSymbolsRequest, SymbolIndex, SymbolSearcher,
};

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
    #[allow(dead_code)]
    symbol_index: Arc<SymbolIndex>,
    symbol_searcher: Arc<SymbolSearcher>,
    root_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CodeRagServer {
    /// Create a new CodeRAG MCP server
    pub fn new(
        search_engine: Arc<SearchEngine>,
        storage: Arc<Storage>,
        symbol_index: Arc<SymbolIndex>,
        root_path: PathBuf,
    ) -> Self {
        let symbol_searcher = Arc::new(SymbolSearcher::new(
            symbol_index.clone(),
            search_engine.clone(),
            storage.clone(),
        ));

        Self {
            search_engine,
            storage,
            symbol_index,
            symbol_searcher,
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

                // Include file header if available
                if let Some(ref header) = result.file_header {
                    output.push_str("\n**File Header (first 50 lines):**\n");
                    output.push_str("```\n");
                    output.push_str(header);
                    if !header.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str("```\n\n");
                    output.push_str("**Matched Chunk:**\n");
                }

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

    /// Find symbol definitions by name
    #[tool(
        name = "find_symbol",
        description = "Find symbol definitions (functions, classes, structs) by name. Supports exact, prefix, and fuzzy matching."
    )]
    async fn find_symbol(
        &self,
        Parameters(req): Parameters<FindSymbolRequest>,
    ) -> Result<CallToolResult, McpError> {
        let response = self
            .symbol_searcher
            .find_symbol(req)
            .await
            .map_err(|e| McpError::internal_error(format!("Symbol search failed: {}", e), None))?;

        let mut output = String::new();
        output.push_str(&format!(
            "# Found {} symbols (mode: {})\n\n",
            response.total_matches, response.search_mode_used
        ));

        for symbol in response.symbols {
            output.push_str(&format!(
                "## {} `{}`\n",
                symbol.kind.to_uppercase(),
                symbol.name
            ));
            output.push_str(&format!("**File:** {}\n", symbol.file_path));
            output.push_str(&format!("**Lines:** {}-{}\n", symbol.start_line, symbol.end_line));

            if let Some(sig) = symbol.signature {
                output.push_str(&format!("**Signature:** `{}`\n", sig));
            }
            if let Some(parent) = symbol.parent {
                output.push_str(&format!("**Parent:** {}\n", parent));
            }
            if let Some(vis) = symbol.visibility {
                output.push_str(&format!("**Visibility:** {}\n", vis));
            }
            output.push_str(&format!("**Relevance:** {:.2}\n\n", symbol.relevance_score));
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// List all symbols in a file or matching criteria
    #[tool(
        name = "list_symbols",
        description = "List all symbols in a file or matching criteria. Can filter by kind and visibility."
    )]
    async fn list_symbols(
        &self,
        Parameters(req): Parameters<ListSymbolsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let response = self
            .symbol_searcher
            .list_symbols(req)
            .await
            .map_err(|e| McpError::internal_error(format!("Symbol listing failed: {}", e), None))?;

        let mut output = String::new();

        if let Some(ref path) = response.file_path {
            output.push_str(&format!("# Symbols in {}\n\n", path));
        } else {
            output.push_str("# All Symbols\n\n");
        }

        output.push_str(&format!("**Total:** {} symbols\n\n", response.total_symbols));

        if let Some(ref by_kind) = response.by_kind {
            // Group by kind
            for (kind, symbols) in by_kind {
                output.push_str(&format!("## {} ({})\n", kind.to_uppercase(), symbols.len()));
                for symbol in symbols {
                    output.push_str(&format!("- **{}** (line {})", symbol.name, symbol.line));
                    if let Some(ref sig) = symbol.signature {
                        output.push_str(&format!(" - `{}`", sig));
                    }
                    output.push('\n');
                }
                output.push('\n');
            }
        } else {
            // Flat list
            for symbol in response.symbols {
                output.push_str(&format!(
                    "- **{}** {} (line {})",
                    symbol.kind, symbol.name, symbol.line
                ));
                if let Some(ref sig) = symbol.signature {
                    output.push_str(&format!(" - `{}`", sig));
                }
                output.push('\n');
            }
        }

        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    /// Find all references to a symbol
    #[tool(
        name = "find_references",
        description = "Find all references to a symbol (basic text search). Returns locations where the symbol is used."
    )]
    async fn find_references(
        &self,
        Parameters(req): Parameters<FindReferencesRequest>,
    ) -> Result<CallToolResult, McpError> {
        let symbol_name = req.symbol_name.clone();
        let response = self
            .symbol_searcher
            .find_references(req)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Reference search failed: {}", e), None)
            })?;

        let mut output = String::new();
        output.push_str(&format!(
            "# References to '{}'\n\n",
            symbol_name
        ));
        output.push_str(&format!(
            "**Found:** {} references in {} files\n\n",
            response.total_references, response.files_affected
        ));

        let mut current_file = String::new();
        for reference in response.references {
            if reference.file_path != current_file {
                current_file = reference.file_path.clone();
                output.push_str(&format!("## {}\n", current_file));
            }

            output.push_str(&format!(
                "- Line {}: `{}`\n",
                reference.start_line,
                reference.line_content.trim()
            ));
        }

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
                "Use this server when user says 'use coderag' or asks to search, explore, or understand the current codebase.\n\n\
                 Available tools:\n\
                 - search: Find relevant code using natural language queries\n\
                 - find_symbol: Find symbol definitions by name (functions, classes, structs)\n\
                 - list_symbols: List all symbols in a file or matching criteria\n\
                 - find_references: Find all references to a symbol\n\
                 - list_files: View all indexed files with optional glob filtering\n\
                 - get_file: Read the full content of any indexed file\n\n\
                 Use 'search' for semantic code discovery. \
                 Use 'find_symbol' to locate specific definitions. \
                 Use 'list_symbols' to explore code structure. \
                 Use 'find_references' to track symbol usage."
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
