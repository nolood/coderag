use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ast_chunker::extractors::SemanticKind;

/// Strategy for chunking code files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChunkerStrategy {
    /// Line-based chunking (traditional approach)
    Line,
    /// AST-based semantic chunking (extracts functions, classes, etc.)
    #[default]
    Ast,
}

impl ChunkerStrategy {
    /// Parse strategy from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "line" => Some(Self::Line),
            "ast" => Some(Self::Ast),
            _ => None,
        }
    }

    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Ast => "ast",
        }
    }
}

/// A chunk of code with metadata about its location
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
    // New fields for v0.2 - AST chunking
    /// Type of semantic unit (None for line-based chunks)
    pub semantic_kind: Option<SemanticKind>,
    /// Name of the unit (function name, class name, etc.)
    pub name: Option<String>,
    /// Signature for functions/methods
    pub signature: Option<String>,
    /// Parent context (class name for methods, impl target for Rust)
    pub parent: Option<String>,
}

/// Splits source code files into chunks suitable for embedding
pub struct Chunker {
    /// Target chunk size in tokens (approximately 4 chars per token)
    chunk_size: usize,
    /// Overlap between chunks in tokens
    overlap: usize,
}

impl Chunker {
    /// Create a new Chunker with the given target chunk size in tokens
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunk_size,
            // 10% overlap by default
            overlap: chunk_size / 10,
        }
    }

    /// Create a chunker with custom overlap
    pub fn with_overlap(chunk_size: usize, overlap: usize) -> Self {
        Self { chunk_size, overlap }
    }

    /// Chunk a file's content into smaller pieces
    ///
    /// Uses line-based chunking that tries to break at natural boundaries
    /// (blank lines, function definitions) when possible.
    pub fn chunk_file(&self, path: &Path, content: &str) -> Vec<Chunk> {
        let language = Self::detect_language(path);
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return Vec::new();
        }

        // Target characters per chunk (approx 4 chars per token)
        let target_chars = self.chunk_size * 4;
        let overlap_chars = self.overlap * 4;

        let mut chunks = Vec::new();
        let mut current_start = 0;

        while current_start < lines.len() {
            let (end_line, chunk_content) =
                self.find_chunk_boundary(&lines, current_start, target_chars);

            if !chunk_content.trim().is_empty() {
                chunks.push(Chunk {
                    content: chunk_content,
                    file_path: path.to_path_buf(),
                    start_line: current_start + 1, // 1-indexed for display
                    end_line: end_line + 1,        // 1-indexed for display
                    language: language.clone(),
                    // Line-based chunks don't have semantic info
                    semantic_kind: None,
                    name: None,
                    signature: None,
                    parent: None,
                });
            }

            // Move start forward, accounting for overlap
            let overlap_lines = self.estimate_overlap_lines(&lines, end_line, overlap_chars);
            current_start = if end_line >= lines.len() - 1 {
                lines.len()
            } else {
                (end_line + 1).saturating_sub(overlap_lines)
            };

            // Prevent infinite loop
            if current_start <= chunks.last().map(|c| c.start_line - 1).unwrap_or(0) {
                current_start = end_line + 1;
            }
        }

        chunks
    }

    /// Find a good chunk boundary starting from `start_line`
    fn find_chunk_boundary(
        &self,
        lines: &[&str],
        start_line: usize,
        target_chars: usize,
    ) -> (usize, String) {
        let mut char_count = 0;
        let mut end_line = start_line;
        let mut last_good_break = start_line;

        for (i, line) in lines.iter().enumerate().skip(start_line) {
            char_count += line.len() + 1; // +1 for newline

            // Track good break points (blank lines, function boundaries)
            if Self::is_good_break_point(line, lines.get(i + 1).copied()) {
                last_good_break = i;
            }

            if char_count >= target_chars {
                // Try to break at a good boundary if within 20% of target
                if last_good_break > start_line && last_good_break >= (i.saturating_sub(i / 5)) {
                    end_line = last_good_break;
                } else {
                    end_line = i;
                }
                break;
            }
            end_line = i;
        }

        let content = lines[start_line..=end_line].join("\n");
        (end_line, content)
    }

    /// Check if this is a good place to break a chunk
    fn is_good_break_point(line: &str, next_line: Option<&str>) -> bool {
        let trimmed = line.trim();

        // Blank line is a good break
        if trimmed.is_empty() {
            return true;
        }

        // End of block (closing brace alone)
        if trimmed == "}" || trimmed == "};" {
            return true;
        }

        // Check if next line starts a new function/class/struct
        if let Some(next) = next_line {
            let next_trimmed = next.trim();
            if next_trimmed.starts_with("fn ")
                || next_trimmed.starts_with("pub fn ")
                || next_trimmed.starts_with("async fn ")
                || next_trimmed.starts_with("pub async fn ")
                || next_trimmed.starts_with("impl ")
                || next_trimmed.starts_with("struct ")
                || next_trimmed.starts_with("pub struct ")
                || next_trimmed.starts_with("enum ")
                || next_trimmed.starts_with("pub enum ")
                || next_trimmed.starts_with("trait ")
                || next_trimmed.starts_with("pub trait ")
                || next_trimmed.starts_with("mod ")
                || next_trimmed.starts_with("pub mod ")
                || next_trimmed.starts_with("def ")
                || next_trimmed.starts_with("async def ")
                || next_trimmed.starts_with("class ")
                || next_trimmed.starts_with("function ")
                || next_trimmed.starts_with("export function ")
                || next_trimmed.starts_with("export const ")
                || next_trimmed.starts_with("export default ")
            {
                return true;
            }
        }

        false
    }

    /// Estimate how many lines to overlap based on target overlap characters
    fn estimate_overlap_lines(&self, lines: &[&str], end_line: usize, overlap_chars: usize) -> usize {
        let mut chars = 0;
        let mut count = 0;

        for i in (0..=end_line).rev() {
            chars += lines[i].len() + 1;
            count += 1;
            if chars >= overlap_chars {
                break;
            }
        }

        count.min(5) // Cap overlap at 5 lines
    }

    /// Detect programming language from file extension
    fn detect_language(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| match ext {
                "rs" => "rust",
                "py" => "python",
                "js" => "javascript",
                "jsx" => "javascript",
                "ts" => "typescript",
                "tsx" => "typescript",
                "go" => "go",
                "java" => "java",
                "c" => "c",
                "cpp" | "cc" | "cxx" => "cpp",
                "h" | "hpp" => "cpp",
                "rb" => "ruby",
                "php" => "php",
                "swift" => "swift",
                "kt" | "kts" => "kotlin",
                "scala" => "scala",
                "cs" => "csharp",
                _ => ext,
            })
            .map(String::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_small_file() {
        let chunker = Chunker::new(512);
        let content = "fn main() {\n    println!(\"Hello\");\n}";
        let path = Path::new("test.rs");

        let chunks = chunker.chunk_file(path, content);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, content);
        assert_eq!(chunks[0].start_line, 1);
        assert_eq!(chunks[0].end_line, 3);
        assert_eq!(chunks[0].language, Some("rust".to_string()));
    }

    #[test]
    fn test_chunk_large_file() {
        let chunker = Chunker::new(50); // Small chunk size to force multiple chunks
        let content = (0..100)
            .map(|i| format!("fn func_{}() {{ /* body */ }}", i))
            .collect::<Vec<_>>()
            .join("\n\n");
        let path = Path::new("test.rs");

        let chunks = chunker.chunk_file(path, &content);

        assert!(chunks.len() > 1);
        // Verify all content is covered
        for chunk in &chunks {
            assert!(!chunk.content.is_empty());
            assert!(chunk.start_line >= 1);
            assert!(chunk.end_line >= chunk.start_line);
        }
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(
            Chunker::detect_language(Path::new("main.rs")),
            Some("rust".to_string())
        );
        assert_eq!(
            Chunker::detect_language(Path::new("script.py")),
            Some("python".to_string())
        );
        assert_eq!(
            Chunker::detect_language(Path::new("app.tsx")),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_empty_file() {
        let chunker = Chunker::new(512);
        let chunks = chunker.chunk_file(Path::new("empty.rs"), "");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_whitespace_only_file() {
        let chunker = Chunker::new(512);
        let chunks = chunker.chunk_file(Path::new("blank.rs"), "   \n\n   \n");
        assert!(chunks.is_empty());
    }
}
