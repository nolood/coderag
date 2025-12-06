//! Symbol search implementation for MCP tools

use anyhow::Result;
use glob::Pattern;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::debug;

use super::index::{SymbolIndex, SymbolRef};
use crate::search::{SearchEngine, traits::Search};
use crate::storage::Storage;

/// Request for finding symbol definitions
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindSymbolRequest {
    /// Symbol name or pattern to search for
    pub query: String,
    /// Filter by semantic kind (function, class, struct, etc.)
    pub kind: Option<String>,
    /// Filter by programming language
    pub language: Option<String>,
    /// Filter by file path pattern (glob)
    pub file_pattern: Option<String>,
    /// Search mode: 'exact', 'prefix', 'fuzzy' (default: 'fuzzy')
    pub mode: Option<String>,
    /// Maximum number of results (default: 20)
    pub limit: Option<usize>,
}

/// Response for symbol search
#[derive(Debug, Serialize)]
pub struct FindSymbolResponse {
    pub symbols: Vec<SymbolResult>,
    pub total_matches: usize,
    pub search_mode_used: String,
}

/// Individual symbol result
#[derive(Debug, Serialize, Clone)]
pub struct SymbolResult {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub parent: Option<String>,
    pub visibility: Option<String>,
    pub relevance_score: f32,
}

/// Request for listing symbols
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSymbolsRequest {
    /// File path to list symbols from
    pub file_path: Option<String>,
    /// Filter by symbol kind
    pub kind_filter: Option<Vec<String>>,
    /// Filter by visibility (public, private, etc.)
    pub visibility: Option<String>,
}

/// Response for listing symbols
#[derive(Debug, Serialize)]
pub struct ListSymbolsResponse {
    pub file_path: Option<String>,
    pub symbols: Vec<SymbolSummary>,
    pub total_symbols: usize,
    pub by_kind: Option<HashMap<String, Vec<SymbolSummary>>>,
}

/// Symbol summary for listing
#[derive(Debug, Serialize, Clone)]
pub struct SymbolSummary {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub signature: Option<String>,
    pub parent: Option<String>,
}

/// Request for finding references to a symbol
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencesRequest {
    /// Symbol name to find references for
    pub symbol_name: String,
    /// File path where symbol is defined (for disambiguation)
    pub file_path: Option<String>,
    /// Maximum number of results (default: 50)
    pub limit: Option<usize>,
}

/// Response for finding references
#[derive(Debug, Serialize)]
pub struct FindReferencesResponse {
    pub references: Vec<ReferenceResult>,
    pub total_references: usize,
    pub files_affected: usize,
}

/// Individual reference result
#[derive(Debug, Serialize)]
pub struct ReferenceResult {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub line_content: String,
    pub match_context: String,
}

/// Symbol searcher that combines index lookups with semantic search
pub struct SymbolSearcher {
    symbol_index: Arc<SymbolIndex>,
    search_engine: Arc<SearchEngine>,
    #[allow(dead_code)]
    storage: Arc<Storage>,
}

impl SymbolSearcher {
    /// Create a new symbol searcher
    pub fn new(
        symbol_index: Arc<SymbolIndex>,
        search_engine: Arc<SearchEngine>,
        storage: Arc<Storage>,
    ) -> Self {
        Self {
            symbol_index,
            search_engine,
            storage,
        }
    }

    /// Find symbols based on the request criteria
    pub async fn find_symbol(&self, request: FindSymbolRequest) -> Result<FindSymbolResponse> {
        let mode = request.mode.as_deref().unwrap_or("fuzzy");
        let limit = request.limit.unwrap_or(20);

        debug!(
            "Searching for symbol '{}' with mode '{}', limit {}",
            request.query, mode, limit
        );

        // Get initial results based on search mode
        let mut results: Vec<(SymbolRef, f32)> = match mode {
            "exact" => self
                .symbol_index
                .find_by_name(&request.query)
                .into_iter()
                .map(|s| (s, 1.0))
                .collect(),
            "prefix" => self
                .symbol_index
                .find_by_prefix(&request.query)
                .into_iter()
                .map(|s| (s, 0.9))
                .collect(),
            "fuzzy" | _ => {
                // Fuzzy search with max distance of 3
                self.symbol_index
                    .find_fuzzy(&request.query, 3)
                    .into_iter()
                    .map(|(s, dist)| {
                        // Convert distance to score (lower distance = higher score)
                        let score = 1.0 / (1.0 + dist as f32);
                        (s, score)
                    })
                    .collect()
            }
        };

        // Apply filters
        if let Some(ref kind) = request.kind {
            results.retain(|(s, _)| s.kind.eq_ignore_ascii_case(kind));
        }

        if let Some(ref pattern) = request.file_pattern {
            let glob = Pattern::new(pattern)?;
            results.retain(|(s, _)| glob.matches(&s.file_path));
        }

        // Sort by relevance score
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Limit results
        results.truncate(limit);

        // Convert to response format
        let symbols: Vec<SymbolResult> = results
            .iter()
            .map(|(s, score)| SymbolResult {
                name: s.name.clone(),
                kind: s.kind.clone(),
                file_path: s.file_path.clone(),
                start_line: s.start_line,
                end_line: s.end_line,
                signature: s.signature.clone(),
                parent: s.parent.clone(),
                visibility: s.visibility.clone(),
                relevance_score: *score,
            })
            .collect();

        let total_matches = symbols.len();

        Ok(FindSymbolResponse {
            symbols,
            total_matches,
            search_mode_used: mode.to_string(),
        })
    }

    /// List symbols in a file or matching criteria
    pub async fn list_symbols(&self, request: ListSymbolsRequest) -> Result<ListSymbolsResponse> {
        let mut symbols: Vec<SymbolRef> = if let Some(ref file_path) = request.file_path {
            self.symbol_index.get_by_file(file_path)
        } else {
            // Get all symbols
            let mut all = Vec::new();
            for file in self.symbol_index.list_files() {
                all.extend(self.symbol_index.get_by_file(&file));
            }
            all
        };

        // Apply filters
        if let Some(ref kinds) = request.kind_filter {
            symbols.retain(|s| kinds.iter().any(|k| s.kind.eq_ignore_ascii_case(k)));
        }

        if let Some(ref visibility) = request.visibility {
            symbols.retain(|s| {
                s.visibility
                    .as_ref()
                    .map_or(false, |v| v.eq_ignore_ascii_case(visibility))
            });
        }

        // Sort by line number
        symbols.sort_by_key(|s| s.start_line);

        // Convert to summary format
        let summaries: Vec<SymbolSummary> = symbols
            .iter()
            .map(|s| SymbolSummary {
                name: s.name.clone(),
                kind: s.kind.clone(),
                line: s.start_line,
                signature: s.signature.clone(),
                parent: s.parent.clone(),
            })
            .collect();

        // Group by kind if requested
        let by_kind = if request.kind_filter.is_some() {
            let mut grouped: HashMap<String, Vec<SymbolSummary>> = HashMap::new();
            for summary in &summaries {
                grouped
                    .entry(summary.kind.clone())
                    .or_insert_with(Vec::new)
                    .push(summary.clone());
            }
            Some(grouped)
        } else {
            None
        };

        Ok(ListSymbolsResponse {
            file_path: request.file_path,
            total_symbols: summaries.len(),
            symbols: summaries,
            by_kind,
        })
    }

    /// Find references to a symbol (basic text search implementation)
    pub async fn find_references(
        &self,
        request: FindReferencesRequest,
    ) -> Result<FindReferencesResponse> {
        let limit = request.limit.unwrap_or(50);

        debug!(
            "Finding references to '{}' with limit {}",
            request.symbol_name, limit
        );

        // For now, use simple text search for the symbol name
        // In a more advanced implementation, we would use AST-based analysis
        let search_results = self
            .search_engine
            .search(&request.symbol_name, limit)
            .await?;

        let mut references = Vec::new();
        let mut files_affected = std::collections::HashSet::new();

        for result in search_results {
            // Skip the definition itself if file_path is provided
            if let Some(ref def_path) = request.file_path {
                if result.file_path == *def_path {
                    continue;
                }
            }

            // Extract the line containing the reference
            let lines: Vec<&str> = result.content.lines().collect();
            let match_line = if !lines.is_empty() {
                lines[0].to_string()
            } else {
                String::new()
            };

            files_affected.insert(result.file_path.clone());

            references.push(ReferenceResult {
                file_path: result.file_path,
                start_line: result.start_line,
                end_line: result.end_line,
                line_content: match_line,
                match_context: result.content,
            });
        }

        Ok(FindReferencesResponse {
            total_references: references.len(),
            files_affected: files_affected.len(),
            references,
        })
    }
}