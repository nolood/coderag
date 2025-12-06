//! Semantic extractors for different programming languages.
//!
//! Each extractor knows how to traverse a language's AST and extract
//! meaningful semantic units like functions, classes, and structs.

pub mod c;
pub mod cpp;
pub mod go;
pub mod java;
pub mod python;
pub mod rust;
pub mod typescript;

use std::collections::HashMap;

use tree_sitter::Tree;

pub use c::CExtractor;
pub use cpp::CppExtractor;
pub use go::GoExtractor;
pub use java::JavaExtractor;
pub use python::PythonExtractor;
pub use rust::RustExtractor;
pub use typescript::TypeScriptExtractor;

/// Types of semantic units we can extract from code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticKind {
    /// A standalone function
    Function,
    /// A method within a class or impl block
    Method,
    /// A struct definition (Rust, Go)
    Struct,
    /// A class definition (Python, Java, TypeScript)
    Class,
    /// A trait definition (Rust)
    Trait,
    /// An interface definition (TypeScript, Go, Java)
    Interface,
    /// An enum definition
    Enum,
    /// An impl block (Rust)
    Impl,
    /// A module/namespace
    Module,
    /// A constant definition
    Constant,
    /// A type alias
    TypeAlias,
    /// A macro definition (Rust)
    Macro,
    /// A test function
    Test,
    /// Fallback for unrecognized but complete blocks
    Block,
}

impl SemanticKind {
    /// Convert to a string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            SemanticKind::Function => "function",
            SemanticKind::Method => "method",
            SemanticKind::Struct => "struct",
            SemanticKind::Class => "class",
            SemanticKind::Trait => "trait",
            SemanticKind::Interface => "interface",
            SemanticKind::Enum => "enum",
            SemanticKind::Impl => "impl",
            SemanticKind::Module => "module",
            SemanticKind::Constant => "constant",
            SemanticKind::TypeAlias => "type_alias",
            SemanticKind::Macro => "macro",
            SemanticKind::Test => "test",
            SemanticKind::Block => "block",
        }
    }

    /// Parse from a string representation.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "function" => Some(SemanticKind::Function),
            "method" => Some(SemanticKind::Method),
            "struct" => Some(SemanticKind::Struct),
            "class" => Some(SemanticKind::Class),
            "trait" => Some(SemanticKind::Trait),
            "interface" => Some(SemanticKind::Interface),
            "enum" => Some(SemanticKind::Enum),
            "impl" => Some(SemanticKind::Impl),
            "module" => Some(SemanticKind::Module),
            "constant" => Some(SemanticKind::Constant),
            "type_alias" => Some(SemanticKind::TypeAlias),
            "macro" => Some(SemanticKind::Macro),
            "test" => Some(SemanticKind::Test),
            "block" => Some(SemanticKind::Block),
            _ => None,
        }
    }
}

/// A semantic code unit extracted from AST.
#[derive(Debug, Clone)]
pub struct SemanticUnit {
    /// The type of semantic unit
    pub kind: SemanticKind,
    /// The name/identifier of the unit (if applicable)
    pub name: Option<String>,
    /// The full source code of this unit
    pub content: String,
    /// Documentation/comments associated with this unit
    pub docs: Option<String>,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed)
    pub end_line: usize,
    /// Start byte offset in source
    pub start_byte: usize,
    /// End byte offset in source
    pub end_byte: usize,
    /// Signature or type information (for functions, methods)
    pub signature: Option<String>,
    /// Parent context (e.g., class name for methods, impl target for Rust)
    pub parent: Option<String>,
}

/// Trait for language-specific AST extractors.
///
/// Each implementation knows how to extract semantic units from
/// a specific language's AST.
pub trait SemanticExtractor: Send + Sync {
    /// Get the language identifier string (e.g., "rust", "python").
    fn language_id(&self) -> &'static str;

    /// Extract semantic units from a parsed tree.
    ///
    /// # Arguments
    /// * `tree` - The parsed tree-sitter Tree
    /// * `source` - The original source code bytes
    ///
    /// # Returns
    /// Vector of extracted semantic units, ordered by their position in the source.
    fn extract(&self, tree: &Tree, source: &[u8]) -> Vec<SemanticUnit>;

    /// Get the node types this extractor looks for.
    ///
    /// This is informational and can be used for debugging.
    fn target_node_types(&self) -> &[&'static str];
}

/// Registry of language-specific extractors.
pub struct ExtractorRegistry {
    extractors: HashMap<String, Box<dyn SemanticExtractor>>,
}

impl ExtractorRegistry {
    /// Create a registry with all built-in extractors.
    pub fn new() -> Self {
        let mut registry = Self {
            extractors: HashMap::new(),
        };

        // Register all built-in extractors
        registry.register(Box::new(RustExtractor));
        registry.register(Box::new(PythonExtractor));
        registry.register(Box::new(TypeScriptExtractor::new(false))); // JavaScript/TypeScript
        registry.register(Box::new(GoExtractor));
        registry.register(Box::new(JavaExtractor));
        registry.register(Box::new(CExtractor));
        registry.register(Box::new(CppExtractor));

        // Also register JavaScript separately (uses same extractor)
        registry.extractors.insert(
            "javascript".to_string(),
            Box::new(TypeScriptExtractor::new(true)),
        );

        registry
    }

    /// Get an extractor for a language.
    pub fn get(&self, language: &str) -> Option<&dyn SemanticExtractor> {
        self.extractors.get(language).map(|e| e.as_ref())
    }

    /// Register a custom extractor.
    pub fn register(&mut self, extractor: Box<dyn SemanticExtractor>) {
        let lang_id = extractor.language_id().to_string();
        self.extractors.insert(lang_id, extractor);
    }

    /// Get a list of supported languages.
    pub fn supported_languages(&self) -> Vec<&str> {
        self.extractors.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to get text from source bytes at a node's range.
pub fn get_node_text(source: &[u8], start: usize, end: usize) -> &str {
    std::str::from_utf8(&source[start..end]).unwrap_or("")
}

/// Helper function to extract a node's text content.
pub fn node_text<'a>(node: &tree_sitter::Node, source: &'a [u8]) -> &'a str {
    get_node_text(source, node.start_byte(), node.end_byte())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_kind_as_str() {
        assert_eq!(SemanticKind::Function.as_str(), "function");
        assert_eq!(SemanticKind::Class.as_str(), "class");
        assert_eq!(SemanticKind::Impl.as_str(), "impl");
    }

    #[test]
    fn test_semantic_kind_from_str() {
        assert_eq!(SemanticKind::parse("function"), Some(SemanticKind::Function));
        assert_eq!(SemanticKind::parse("class"), Some(SemanticKind::Class));
        assert_eq!(SemanticKind::parse("unknown"), None);
    }

    #[test]
    fn test_registry_creation() {
        let registry = ExtractorRegistry::new();
        let languages = registry.supported_languages();

        assert!(languages.contains(&"rust"));
        assert!(languages.contains(&"python"));
        assert!(languages.contains(&"typescript"));
        assert!(languages.contains(&"javascript"));
        assert!(languages.contains(&"go"));
        assert!(languages.contains(&"java"));
    }

    #[test]
    fn test_registry_get() {
        let registry = ExtractorRegistry::new();

        assert!(registry.get("rust").is_some());
        assert!(registry.get("python").is_some());
        assert!(registry.get("unknown").is_none());
    }
}
