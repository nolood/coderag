//! Symbol indexing and search module
//!
//! This module provides in-memory indexing of code symbols (functions, classes, structs, etc.)
//! extracted during the AST chunking process. It enables fast symbol lookup and search
//! for MCP tools.

pub mod index;
pub mod search;

pub use index::{SymbolIndex, SymbolRef};
pub use search::{FindSymbolRequest, FindReferencesRequest, ListSymbolsRequest, SymbolSearcher};