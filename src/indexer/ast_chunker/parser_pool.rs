//! Thread-safe parser pool for tree-sitter parsers.
//!
//! Provides parser management with language detection and grammar initialization.

use std::collections::HashMap;

use tree_sitter::{Language, Parser};
use tracing::debug;

/// Manages tree-sitter parsers for multiple languages.
///
/// Each language gets its own parser instance, which is reused
/// across multiple parse operations.
pub struct ParserPool {
    /// Map of language identifier to configured parser
    parsers: HashMap<String, Parser>,
    /// Map of language identifier to tree-sitter Language
    languages: HashMap<String, Language>,
}

impl ParserPool {
    /// Create a new parser pool with all supported languages.
    pub fn new() -> Self {
        let mut pool = Self {
            parsers: HashMap::new(),
            languages: HashMap::new(),
        };

        // Register all supported languages
        pool.register_language("rust", tree_sitter_rust::LANGUAGE.into());
        pool.register_language("python", tree_sitter_python::LANGUAGE.into());
        pool.register_language("javascript", tree_sitter_javascript::LANGUAGE.into());
        pool.register_language("typescript", tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
        pool.register_language("tsx", tree_sitter_typescript::LANGUAGE_TSX.into());
        pool.register_language("go", tree_sitter_go::LANGUAGE.into());
        pool.register_language("java", tree_sitter_java::LANGUAGE.into());
        pool.register_language("c", tree_sitter_c::LANGUAGE.into());
        pool.register_language("cpp", tree_sitter_cpp::LANGUAGE.into());

        pool
    }

    /// Register a language with its tree-sitter grammar.
    fn register_language(&mut self, id: &str, language: Language) {
        self.languages.insert(id.to_string(), language);
    }

    /// Get a parser for the given language.
    ///
    /// Creates the parser on first access and caches it for reuse.
    /// Returns `None` if the language is not supported.
    pub fn get_parser(&mut self, language: &str) -> Option<&mut Parser> {
        // If parser already exists, return it
        if self.parsers.contains_key(language) {
            return self.parsers.get_mut(language);
        }

        // Try to create a new parser for this language
        let ts_language = self.languages.get(language)?;

        let mut parser = Parser::new();
        if let Err(e) = parser.set_language(ts_language) {
            debug!("Failed to set language '{}' for parser: {:?}", language, e);
            return None;
        }

        self.parsers.insert(language.to_string(), parser);
        self.parsers.get_mut(language)
    }

    /// Check if a language is supported.
    pub fn supports(&self, language: &str) -> bool {
        self.languages.contains_key(language)
    }

    /// Get a list of all supported languages.
    pub fn supported_languages(&self) -> Vec<&str> {
        self.languages.keys().map(|s| s.as_str()).collect()
    }

    /// Get the tree-sitter Language for a language identifier.
    pub fn get_language(&self, language: &str) -> Option<&Language> {
        self.languages.get(language)
    }

    /// Detect language from file extension and return the appropriate language ID.
    pub fn detect_language_from_extension(ext: &str) -> Option<&'static str> {
        match ext {
            "rs" => Some("rust"),
            "py" => Some("python"),
            "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
            "ts" => Some("typescript"),
            "tsx" => Some("tsx"),
            "go" => Some("go"),
            "java" => Some("java"),
            "c" => Some("c"),
            "cc" | "cxx" | "cpp" | "c++" => Some("cpp"),
            "h" => Some("c"),  // Default .h to C
            "hpp" | "hxx" | "h++" | "hh" => Some("cpp"),
            _ => None,
        }
    }
}

impl Default for ParserPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_languages() {
        let pool = ParserPool::new();
        let languages = pool.supported_languages();

        assert!(languages.contains(&"rust"));
        assert!(languages.contains(&"python"));
        assert!(languages.contains(&"javascript"));
        assert!(languages.contains(&"typescript"));
        assert!(languages.contains(&"go"));
        assert!(languages.contains(&"java"));
    }

    #[test]
    fn test_supports() {
        let pool = ParserPool::new();

        assert!(pool.supports("rust"));
        assert!(pool.supports("python"));
        assert!(!pool.supports("cobol"));
    }

    #[test]
    fn test_get_parser() {
        let mut pool = ParserPool::new();

        let parser = pool.get_parser("rust");
        assert!(parser.is_some());

        let parser = pool.get_parser("unknown");
        assert!(parser.is_none());
    }

    #[test]
    fn test_parser_reuse() {
        let mut pool = ParserPool::new();

        // First access creates the parser
        let _ = pool.get_parser("rust");
        assert!(pool.parsers.contains_key("rust"));

        // Second access reuses it
        let parser = pool.get_parser("rust");
        assert!(parser.is_some());
    }

    #[test]
    fn test_detect_language_from_extension() {
        assert_eq!(
            ParserPool::detect_language_from_extension("rs"),
            Some("rust")
        );
        assert_eq!(
            ParserPool::detect_language_from_extension("py"),
            Some("python")
        );
        assert_eq!(
            ParserPool::detect_language_from_extension("ts"),
            Some("typescript")
        );
        assert_eq!(
            ParserPool::detect_language_from_extension("tsx"),
            Some("tsx")
        );
        assert_eq!(
            ParserPool::detect_language_from_extension("txt"),
            None
        );
    }
}
