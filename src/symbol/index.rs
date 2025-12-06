//! In-memory symbol index for fast lookups

use std::collections::HashMap;
use tracing::info;

use crate::storage::IndexedChunk;

/// Reference to a symbol in the index
#[derive(Debug, Clone)]
pub struct SymbolRef {
    /// Unique chunk ID in the database
    pub chunk_id: String,
    /// Symbol name
    pub name: String,
    /// Semantic kind (function, class, struct, etc.)
    pub kind: String,
    /// File path containing the symbol
    pub file_path: String,
    /// Starting line number
    pub start_line: usize,
    /// Ending line number
    pub end_line: usize,
    /// Function/method signature
    pub signature: Option<String>,
    /// Parent context (e.g., class name for methods)
    pub parent: Option<String>,
    /// Visibility modifier
    pub visibility: Option<String>,
}

/// In-memory index for fast symbol lookups
pub struct SymbolIndex {
    /// Index by symbol name
    by_name: HashMap<String, Vec<SymbolRef>>,
    /// Index by semantic kind
    by_kind: HashMap<String, Vec<SymbolRef>>,
    /// Index by file path
    by_file: HashMap<String, Vec<SymbolRef>>,
    /// Total number of indexed symbols
    symbol_count: usize,
}

impl SymbolIndex {
    /// Create a new empty symbol index
    pub fn new() -> Self {
        Self {
            by_name: HashMap::new(),
            by_kind: HashMap::new(),
            by_file: HashMap::new(),
            symbol_count: 0,
        }
    }

    /// Build index from chunks loaded from storage
    pub fn build_from_chunks(chunks: &[IndexedChunk]) -> Self {
        let mut index = Self::new();

        for chunk in chunks {
            // Only process chunks that have symbol information
            if let Some(ref symbol_name) = chunk.symbol_name {
                let symbol_ref = SymbolRef {
                    chunk_id: chunk.id.clone(),
                    name: symbol_name.clone(),
                    kind: chunk.semantic_kind.clone().unwrap_or_else(|| "unknown".to_string()),
                    file_path: chunk.file_path.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    signature: chunk.signature.clone(),
                    parent: chunk.parent.clone(),
                    visibility: chunk.visibility.clone(),
                };

                index.add_symbol(symbol_ref);
            }
        }

        info!(
            "Built symbol index with {} symbols across {} files",
            index.symbol_count,
            index.by_file.len()
        );

        index
    }

    /// Add a symbol to the index
    pub fn add_symbol(&mut self, symbol: SymbolRef) {
        // Index by name
        self.by_name
            .entry(symbol.name.clone())
            .or_default()
            .push(symbol.clone());

        // Index by kind
        self.by_kind
            .entry(symbol.kind.clone())
            .or_default()
            .push(symbol.clone());

        // Index by file
        self.by_file
            .entry(symbol.file_path.clone())
            .or_default()
            .push(symbol.clone());

        self.symbol_count += 1;
    }

    /// Find symbols by exact name
    pub fn find_by_name(&self, name: &str) -> Vec<SymbolRef> {
        self.by_name
            .get(name)
            .map(|refs| refs.clone())
            .unwrap_or_default()
    }

    /// Find symbols by name prefix
    pub fn find_by_prefix(&self, prefix: &str) -> Vec<SymbolRef> {
        let prefix_lower = prefix.to_lowercase();
        let mut results = Vec::new();

        for (name, refs) in &self.by_name {
            if name.to_lowercase().starts_with(&prefix_lower) {
                results.extend(refs.clone());
            }
        }

        results
    }

    /// Find symbols by fuzzy matching
    pub fn find_fuzzy(&self, query: &str, max_distance: usize) -> Vec<(SymbolRef, usize)> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (name, refs) in &self.by_name {
            let name_lower = name.to_lowercase();
            let distance = levenshtein_distance(&query_lower, &name_lower);

            if distance <= max_distance {
                for symbol_ref in refs {
                    results.push((symbol_ref.clone(), distance));
                }
            }
        }

        // Sort by distance (best matches first)
        results.sort_by_key(|(_, dist)| *dist);
        results
    }

    /// Get all symbols of a specific kind
    pub fn get_by_kind(&self, kind: &str) -> Vec<SymbolRef> {
        self.by_kind
            .get(kind)
            .map(|refs| refs.clone())
            .unwrap_or_default()
    }

    /// Get all symbols in a file
    pub fn get_by_file(&self, file_path: &str) -> Vec<SymbolRef> {
        self.by_file
            .get(file_path)
            .map(|refs| refs.clone())
            .unwrap_or_default()
    }

    /// List all files with symbols
    pub fn list_files(&self) -> Vec<String> {
        self.by_file.keys().cloned().collect()
    }

    /// Get all unique symbol kinds
    pub fn list_kinds(&self) -> Vec<String> {
        self.by_kind.keys().cloned().collect()
    }

    /// Get total symbol count
    pub fn symbol_count(&self) -> usize {
        self.symbol_count
    }

    /// Clear the index
    pub fn clear(&mut self) {
        self.by_name.clear();
        self.by_kind.clear();
        self.by_file.clear();
        self.symbol_count = 0;
    }
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate Levenshtein distance between two strings
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut matrix: Vec<Vec<usize>> = vec![vec![0; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };

            matrix[i][j] = std::cmp::min(
                std::cmp::min(
                    matrix[i - 1][j] + 1,      // deletion
                    matrix[i][j - 1] + 1,      // insertion
                ),
                matrix[i - 1][j - 1] + cost,  // substitution
            );
        }
    }

    matrix[a_len][b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
        assert_eq!(levenshtein_distance("hello", "hallo"), 1);
        assert_eq!(levenshtein_distance("hello", "hell"), 1);
        assert_eq!(levenshtein_distance("hello", "world"), 4);
    }

    #[test]
    fn test_symbol_index_basic() {
        let mut index = SymbolIndex::new();

        let symbol = SymbolRef {
            chunk_id: "test-id".to_string(),
            name: "test_function".to_string(),
            kind: "function".to_string(),
            file_path: "src/test.rs".to_string(),
            start_line: 10,
            end_line: 20,
            signature: Some("fn test_function() -> Result<()>".to_string()),
            parent: None,
            visibility: Some("pub".to_string()),
        };

        index.add_symbol(symbol.clone());

        assert_eq!(index.symbol_count(), 1);
        assert_eq!(index.find_by_name("test_function").len(), 1);
        assert_eq!(index.get_by_kind("function").len(), 1);
        assert_eq!(index.get_by_file("src/test.rs").len(), 1);
    }

    #[test]
    fn test_prefix_search() {
        let mut index = SymbolIndex::new();

        for name in &["test_function", "test_method", "other_function"] {
            index.add_symbol(SymbolRef {
                chunk_id: format!("id-{}", name),
                name: name.to_string(),
                kind: "function".to_string(),
                file_path: "test.rs".to_string(),
                start_line: 1,
                end_line: 2,
                signature: None,
                parent: None,
                visibility: None,
            });
        }

        let results = index.find_by_prefix("test");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_fuzzy_search() {
        let mut index = SymbolIndex::new();

        index.add_symbol(SymbolRef {
            chunk_id: "id-1".to_string(),
            name: "hello_world".to_string(),
            kind: "function".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 1,
            end_line: 2,
            signature: None,
            parent: None,
            visibility: None,
        });

        let results = index.find_fuzzy("helo_world", 2);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, 1); // Distance of 1
    }
}