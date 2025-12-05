//! MCP (Model Context Protocol) server module for CodeRAG.
//!
//! This module provides an MCP server that exposes semantic code search
//! capabilities to LLM clients like Claude.
//!
//! ## Transport Options
//!
//! The MCP server supports two transport modes:
//!
//! - **stdio**: Standard input/output transport for local CLI usage (default)
//! - **http**: HTTP/SSE transport for remote access and multiple clients
//!
//! ## Usage
//!
//! ```ignore
//! use coderag::mcp::{CodeRagServer, Transport, http::run_http_server};
//!
//! // Stdio transport (default)
//! let server = CodeRagServer::new(search_engine, storage, root_path);
//! server.run().await?;
//!
//! // HTTP transport
//! run_http_server(search_engine, storage, root_path, 3000).await?;
//! ```

pub mod http;
mod server;

pub use http::{run_http_server, HttpTransport, HttpTransportConfig};
pub use server::CodeRagServer;

/// MCP transport type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    /// Standard input/output transport (default)
    #[default]
    Stdio,
    /// HTTP/SSE transport for remote access
    Http,
}

impl Transport {
    /// Parse transport type from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "stdio" => Some(Self::Stdio),
            "http" | "sse" | "http-sse" => Some(Self::Http),
            _ => None,
        }
    }

    /// Get the transport type as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stdio => "stdio",
            Self::Http => "http",
        }
    }
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Transport {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("Unknown transport type: {}", s))
    }
}
