//! C-specific semantic extractor.
//!
//! Extracts: function_definition, struct_specifier, union_specifier, enum_specifier,
//! type_definition

use tree_sitter::{Node, Tree, TreeCursor};

use super::{node_text, SemanticExtractor, SemanticKind, SemanticUnit};

/// C language semantic extractor.
pub struct CExtractor;

impl SemanticExtractor for CExtractor {
    fn language_id(&self) -> &'static str {
        "c"
    }

    fn extract(&self, tree: &Tree, source: &[u8]) -> Vec<SemanticUnit> {
        let mut units = Vec::new();
        let mut cursor = tree.walk();

        self.extract_from_node(&mut cursor, source, &mut units);

        // Sort by start position
        units.sort_by_key(|u| (u.start_line, u.start_byte));

        units
    }

    fn target_node_types(&self) -> &[&'static str] {
        &[
            "function_definition",
            "declaration",        // For static inline functions
            "struct_specifier",
            "union_specifier",
            "enum_specifier",
            "type_definition",
        ]
    }
}

impl CExtractor {
    /// Recursively extract semantic units from the AST.
    fn extract_from_node(
        &self,
        cursor: &mut TreeCursor,
        source: &[u8],
        units: &mut Vec<SemanticUnit>,
    ) {
        let node = cursor.node();

        // Try to extract a semantic unit from this node
        if let Some(unit) = self.extract_unit(&node, source) {
            units.push(unit);
        }

        // Recurse into children
        if cursor.goto_first_child() {
            loop {
                self.extract_from_node(cursor, source, units);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    /// Extract a semantic unit from a node if it's a target type.
    fn extract_unit(
        &self,
        node: &Node,
        source: &[u8],
    ) -> Option<SemanticUnit> {
        let kind = match node.kind() {
            "function_definition" => SemanticKind::Function,
            "struct_specifier" => SemanticKind::Struct,
            "union_specifier" => SemanticKind::Struct, // Treat unions similarly to structs
            "enum_specifier" => SemanticKind::Enum,
            "type_definition" => SemanticKind::TypeAlias,
            _ => return None,
        };

        let name = self.get_name(node, source);
        let signature = self.get_signature(node, source);
        let docs = self.get_docs(node, source);
        let content = node_text(node, source).to_string();

        Some(SemanticUnit {
            kind,
            name,
            content,
            docs,
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            signature,
            parent: None,
        })
    }

    /// Get the name/identifier from a node.
    fn get_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_definition" => {
                // In function_definition, find the function_declarator
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child_node = cursor.node();
                        if child_node.kind() == "function_declarator" {
                            // Find the identifier within function_declarator
                            if let Some(identifier) = self.find_identifier_in_declarator(&child_node, source) {
                                return Some(identifier);
                            }
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                None
            }
            "struct_specifier" | "union_specifier" | "enum_specifier" => {
                // These have a name field
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "type_definition" => {
                // Type definitions have a declarator for the new type name
                node.child_by_field_name("declarator")
                    .and_then(|d| self.find_identifier_in_declarator(&d, source))
            }
            _ => None,
        }
    }

    /// Find identifier within a declarator node (handles nested declarators)
    fn find_identifier_in_declarator(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "identifier" => Some(node_text(node, source).to_string()),
            "function_declarator" | "pointer_declarator" | "array_declarator" => {
                // These contain the identifier as a child
                node.child_by_field_name("declarator")
                    .and_then(|d| self.find_identifier_in_declarator(&d, source))
            }
            _ => {
                // Try to find an identifier child directly
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        if cursor.node().kind() == "identifier" {
                            return Some(node_text(&cursor.node(), source).to_string());
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                None
            }
        }
    }

    /// Get function signature.
    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        if node.kind() != "function_definition" {
            return None;
        }

        // Build signature from return type, name, and parameters
        let mut parts = Vec::new();

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "primitive_type" | "type_identifier" | "sized_type_specifier" => {
                        parts.push(node_text(&child, source).to_string());
                    }
                    "pointer_declarator" => {
                        // Include pointer in the signature
                        if parts.is_empty() {
                            parts.push(node_text(&child, source).to_string());
                        }
                    }
                    "function_declarator" => {
                        // Extract function name and parameters
                        parts.push(node_text(&child, source).to_string());
                        break;
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if !parts.is_empty() {
            Some(parts.join(" ").trim().to_string())
        } else {
            None
        }
    }

    /// Get documentation/comments associated with a node.
    fn get_docs(&self, node: &Node, source: &[u8]) -> Option<String> {
        // Look for preceding comments (// or /* */)
        let mut docs = Vec::new();

        // Check previous sibling
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "comment" {
                let comment_text = node_text(&prev, source);
                docs.push(self.clean_comment(comment_text));
            }
        }

        // For multi-line comments, check multiple previous siblings
        let mut current = node.prev_sibling();
        while let Some(prev_node) = current {
            if prev_node.kind() == "comment" {
                let comment_text = node_text(&prev_node, source);
                docs.insert(0, self.clean_comment(comment_text));
                current = prev_node.prev_sibling();
            } else {
                break;
            }
        }

        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }

    /// Clean comment text by removing comment markers
    fn clean_comment(&self, comment: &str) -> String {
        let trimmed = comment.trim();
        if trimmed.starts_with("//") {
            trimmed.trim_start_matches("//").trim().to_string()
        } else if trimmed.starts_with("/*") && trimmed.ends_with("*/") {
            trimmed
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim()
                .to_string()
        } else {
            trimmed.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_c(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .expect("Failed to set C language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
int add(int a, int b) {
    return a + b;
}
"#;
        let tree = parse_c(source);
        let extractor = CExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert_eq!(units[0].name, Some("add".to_string()));
        assert!(units[0].signature.is_some());
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
struct Point {
    int x;
    int y;
};
"#;
        let tree = parse_c(source);
        let extractor = CExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Struct);
        assert_eq!(units[0].name, Some("Point".to_string()));
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
enum Color {
    RED,
    GREEN,
    BLUE
};
"#;
        let tree = parse_c(source);
        let extractor = CExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Enum);
        assert_eq!(units[0].name, Some("Color".to_string()));
    }

    #[test]
    fn test_extract_typedef() {
        let source = r#"
typedef struct {
    int x;
    int y;
} Point;
"#;
        let tree = parse_c(source);
        let extractor = CExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        // Should find the typedef
        assert!(units.iter().any(|u| u.kind == SemanticKind::TypeAlias));
    }

    #[test]
    fn test_extract_with_comments() {
        let source = r#"
// This function adds two numbers
int add(int a, int b) {
    return a + b;
}

/* Multi-line comment
   for subtract function */
int subtract(int a, int b) {
    return a - b;
}
"#;
        let tree = parse_c(source);
        let extractor = CExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 2);

        // Check first function has docs
        let add_func = units.iter().find(|u| u.name == Some("add".to_string())).unwrap();
        assert!(add_func.docs.is_some());

        // Check second function has docs
        let sub_func = units.iter().find(|u| u.name == Some("subtract".to_string())).unwrap();
        assert!(sub_func.docs.is_some());
    }

    #[test]
    fn test_extract_union() {
        let source = r#"
union Data {
    int i;
    float f;
    char str[20];
};
"#;
        let tree = parse_c(source);
        let extractor = CExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Struct); // Unions treated as structs
        assert_eq!(units[0].name, Some("Data".to_string()));
    }

    #[test]
    fn test_extract_pointer_function() {
        let source = r#"
char* get_string(void) {
    return "hello";
}
"#;
        let tree = parse_c(source);
        let extractor = CExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert_eq!(units[0].name, Some("get_string".to_string()));
    }
}