//! AST-based chunking module for semantic code extraction.
//!
//! This module provides tree-sitter based parsing and semantic unit extraction
//! for multiple programming languages. It extracts complete semantic units
//! (functions, classes, structs, etc.) rather than arbitrary line-based chunks.

pub mod extractors;
pub mod parser_pool;

use std::path::Path;

use tracing::{debug, warn};

use crate::indexer::chunker::Chunker;
use crate::indexer::Chunk;

pub use extractors::{ExtractorRegistry, SemanticExtractor, SemanticKind, SemanticUnit};
pub use parser_pool::ParserPool;

/// Statistics from a chunking operation
#[derive(Debug, Default, Clone)]
pub struct ChunkingStats {
    /// Method used for chunking
    pub method_used: ChunkingMethod,
    /// Number of semantic units extracted
    pub semantic_units_extracted: usize,
    /// Number of small units that were merged
    pub units_merged: usize,
    /// Number of fallback (line-based) chunks created
    pub fallback_chunks: usize,
}

/// The method used for chunking a file
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ChunkingMethod {
    /// Line-based chunking (fallback)
    #[default]
    LineBased,
    /// AST-based semantic chunking
    Ast,
    /// Mixed approach (some AST, some line-based)
    Mixed,
}

/// AST-based chunker that extracts semantic units from code.
///
/// Falls back to line-based chunking when:
/// - Language is not supported
/// - Parsing fails
/// - File has no extractable semantic units
pub struct AstChunker {
    /// Pool of tree-sitter parsers
    parser_pool: ParserPool,
    /// Registry of language-specific extractors
    extractors: ExtractorRegistry,
    /// Fallback line-based chunker
    fallback: Chunker,
    /// Minimum chunk size in approximate tokens (smaller units get merged)
    min_chunk_tokens: usize,
    /// Maximum chunk size in approximate tokens (larger units use line chunking)
    max_chunk_tokens: usize,
    /// Statistics from last chunking operation
    last_stats: ChunkingStats,
}

impl AstChunker {
    /// Create a new AST chunker with default configuration.
    ///
    /// Default token limits:
    /// - Minimum: 50 tokens
    /// - Maximum: 1500 tokens
    pub fn new() -> Self {
        Self::with_limits(50, 1500)
    }

    /// Create an AST chunker with custom token limits.
    ///
    /// # Arguments
    /// * `min_tokens` - Minimum tokens for a semantic unit (smaller units get merged)
    /// * `max_tokens` - Maximum tokens for a semantic unit (larger units use line chunking)
    pub fn with_limits(min_tokens: usize, max_tokens: usize) -> Self {
        Self {
            parser_pool: ParserPool::new(),
            extractors: ExtractorRegistry::new(),
            fallback: Chunker::new(max_tokens),
            min_chunk_tokens: min_tokens,
            max_chunk_tokens: max_tokens,
            last_stats: ChunkingStats::default(),
        }
    }

    /// Chunk a file using AST extraction.
    ///
    /// Falls back to line-based chunking if:
    /// - Language is not supported
    /// - Parsing fails
    /// - Extracted units are too large
    ///
    /// # Arguments
    /// * `path` - File path (used for language detection)
    /// * `content` - File content
    ///
    /// # Returns
    /// Vector of chunks, each representing a semantic unit or line-based chunk
    pub fn chunk_file(&mut self, path: &Path, content: &str) -> Vec<Chunk> {
        // Reset stats
        self.last_stats = ChunkingStats::default();

        // Detect language from file extension
        let language = match Self::detect_language(path) {
            Some(lang) => lang,
            None => {
                debug!("Unknown language for {:?}, using line-based chunking", path);
                self.last_stats.method_used = ChunkingMethod::LineBased;
                let chunks = self.fallback.chunk_file(path, content);
                self.last_stats.fallback_chunks = chunks.len();
                return chunks;
            }
        };

        // Check if we have an extractor for this language
        let extractor = match self.extractors.get(&language) {
            Some(ext) => ext,
            None => {
                debug!(
                    "No extractor for language '{}', using line-based chunking",
                    language
                );
                self.last_stats.method_used = ChunkingMethod::LineBased;
                let chunks = self.fallback.chunk_file(path, content);
                self.last_stats.fallback_chunks = chunks.len();
                return chunks;
            }
        };

        // Get or create parser for this language
        let parser = match self.parser_pool.get_parser(&language) {
            Some(p) => p,
            None => {
                warn!(
                    "Failed to get parser for language '{}', using line-based chunking",
                    language
                );
                self.last_stats.method_used = ChunkingMethod::LineBased;
                let chunks = self.fallback.chunk_file(path, content);
                self.last_stats.fallback_chunks = chunks.len();
                return chunks;
            }
        };

        // Parse the content
        let tree = match parser.parse(content.as_bytes(), None) {
            Some(t) => t,
            None => {
                warn!("Failed to parse {:?}, using line-based chunking", path);
                self.last_stats.method_used = ChunkingMethod::LineBased;
                let chunks = self.fallback.chunk_file(path, content);
                self.last_stats.fallback_chunks = chunks.len();
                return chunks;
            }
        };

        // Extract semantic units
        let units = extractor.extract(&tree, content.as_bytes());

        if units.is_empty() {
            debug!(
                "No semantic units found in {:?}, using line-based chunking",
                path
            );
            self.last_stats.method_used = ChunkingMethod::LineBased;
            let chunks = self.fallback.chunk_file(path, content);
            self.last_stats.fallback_chunks = chunks.len();
            return chunks;
        }

        self.last_stats.semantic_units_extracted = units.len();

        // Convert semantic units to chunks, handling merging and splitting
        let chunks = self.process_semantic_units(path, content, units, &language);

        // Determine method used
        if self.last_stats.fallback_chunks > 0 && self.last_stats.semantic_units_extracted > 0 {
            self.last_stats.method_used = ChunkingMethod::Mixed;
        } else if self.last_stats.fallback_chunks > 0 {
            self.last_stats.method_used = ChunkingMethod::LineBased;
        } else {
            self.last_stats.method_used = ChunkingMethod::Ast;
        }

        chunks
    }

    /// Get statistics about the last chunking operation.
    pub fn last_stats(&self) -> &ChunkingStats {
        &self.last_stats
    }

    /// Process semantic units into chunks, handling size constraints.
    fn process_semantic_units(
        &mut self,
        path: &Path,
        content: &str,
        units: Vec<SemanticUnit>,
        language: &str,
    ) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut pending_small_units: Vec<SemanticUnit> = Vec::new();

        for unit in units {
            let token_estimate = Self::estimate_tokens(&unit.content);

            if token_estimate > self.max_chunk_tokens {
                // Unit is too large, use line-based chunking for it
                debug!(
                    "Semantic unit '{}' is too large ({} tokens), using line-based chunking",
                    unit.name.as_deref().unwrap_or("unnamed"),
                    token_estimate
                );

                // First, flush any pending small units
                if !pending_small_units.is_empty() {
                    chunks.push(self.merge_small_units(path, &pending_small_units, language));
                    pending_small_units.clear();
                }

                // Chunk the large unit with line-based chunker
                let unit_chunks = self.fallback.chunk_file(path, &unit.content);
                self.last_stats.fallback_chunks += unit_chunks.len();

                // Adjust line numbers for the sub-chunks
                for mut chunk in unit_chunks {
                    chunk.start_line = unit.start_line + chunk.start_line - 1;
                    chunk.end_line = unit.start_line + chunk.end_line - 1;
                    // Preserve semantic info in the first chunk
                    if chunks.is_empty()
                        || chunks.last().map(|c| c.end_line).unwrap_or(0) < unit.start_line
                    {
                        chunk.semantic_kind = Some(unit.kind);
                        chunk.name = unit.name.clone();
                        chunk.signature = unit.signature.clone();
                        chunk.parent = unit.parent.clone();
                    }
                    chunks.push(chunk);
                }
            } else if token_estimate < self.min_chunk_tokens {
                // Unit is small, accumulate for merging
                pending_small_units.push(unit);

                // Check if accumulated units are large enough
                let total_tokens: usize = pending_small_units
                    .iter()
                    .map(|u| Self::estimate_tokens(&u.content))
                    .sum();

                if total_tokens >= self.min_chunk_tokens {
                    chunks.push(self.merge_small_units(path, &pending_small_units, language));
                    self.last_stats.units_merged += pending_small_units.len();
                    pending_small_units.clear();
                }
            } else {
                // Unit is within acceptable size range
                // First, flush any pending small units
                if !pending_small_units.is_empty() {
                    chunks.push(self.merge_small_units(path, &pending_small_units, language));
                    self.last_stats.units_merged += pending_small_units.len();
                    pending_small_units.clear();
                }

                chunks.push(Chunk {
                    content: unit.content,
                    file_path: path.to_path_buf(),
                    start_line: unit.start_line,
                    end_line: unit.end_line,
                    language: Some(language.to_string()),
                    semantic_kind: Some(unit.kind),
                    name: unit.name,
                    signature: unit.signature,
                    parent: unit.parent,
                });
            }
        }

        // Handle any remaining small units
        if !pending_small_units.is_empty() {
            chunks.push(self.merge_small_units(path, &pending_small_units, language));
            self.last_stats.units_merged += pending_small_units.len();
        }

        // If no chunks were created, fall back to line-based
        if chunks.is_empty() {
            debug!("No valid chunks created, falling back to line-based");
            let fallback_chunks = self.fallback.chunk_file(path, content);
            self.last_stats.fallback_chunks = fallback_chunks.len();
            return fallback_chunks;
        }

        chunks
    }

    /// Merge multiple small semantic units into a single chunk.
    fn merge_small_units(
        &self,
        path: &Path,
        units: &[SemanticUnit],
        language: &str,
    ) -> Chunk {
        let content = units
            .iter()
            .map(|u| u.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let start_line = units.iter().map(|u| u.start_line).min().unwrap_or(1);
        let end_line = units.iter().map(|u| u.end_line).max().unwrap_or(1);

        // Use the first unit's semantic info if it's meaningful
        let first = units.first();

        Chunk {
            content,
            file_path: path.to_path_buf(),
            start_line,
            end_line,
            language: Some(language.to_string()),
            semantic_kind: first.map(|u| u.kind),
            name: first.and_then(|u| u.name.clone()),
            signature: first.and_then(|u| u.signature.clone()),
            parent: first.and_then(|u| u.parent.clone()),
        }
    }

    /// Estimate the number of tokens in a string (approximately 4 chars per token).
    fn estimate_tokens(s: &str) -> usize {
        s.len() / 4
    }

    /// Detect programming language from file extension.
    fn detect_language(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "rs" => Some("rust"),
                "py" => Some("python"),
                "js" => Some("javascript"),
                "jsx" => Some("javascript"),
                "ts" => Some("typescript"),
                "tsx" => Some("typescript"),
                "go" => Some("go"),
                "java" => Some("java"),
                _ => None,
            })
            .map(String::from)
    }
}

impl Default for AstChunker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(
            AstChunker::detect_language(Path::new("main.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            AstChunker::detect_language(Path::new("script.py")),
            Some("python".to_string())
        );
        assert_eq!(
            AstChunker::detect_language(Path::new("app.tsx")),
            Some("typescript".to_string())
        );
        assert_eq!(AstChunker::detect_language(Path::new("file.txt")), None);
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(AstChunker::estimate_tokens(""), 0);
        assert_eq!(AstChunker::estimate_tokens("test"), 1);
        assert_eq!(AstChunker::estimate_tokens("12345678"), 2);
    }

    #[test]
    fn test_chunking_method_default() {
        let method = ChunkingMethod::default();
        assert_eq!(method, ChunkingMethod::LineBased);
    }
}
