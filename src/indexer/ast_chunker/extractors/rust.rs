//! Rust-specific semantic extractor.
//!
//! Extracts: function_item, impl_item, struct_item, enum_item, trait_item,
//! mod_item, const_item, type_item, macro_definition

use tree_sitter::{Node, Tree, TreeCursor};

use super::{node_text, SemanticExtractor, SemanticKind, SemanticUnit};

/// Rust language semantic extractor.
pub struct RustExtractor;

impl SemanticExtractor for RustExtractor {
    fn language_id(&self) -> &'static str {
        "rust"
    }

    fn extract(&self, tree: &Tree, source: &[u8]) -> Vec<SemanticUnit> {
        let mut units = Vec::new();
        let mut cursor = tree.walk();

        self.extract_from_node(&mut cursor, source, &mut units, None);

        // Sort by start position
        units.sort_by_key(|u| (u.start_line, u.start_byte));

        units
    }

    fn target_node_types(&self) -> &[&'static str] {
        &[
            "function_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "impl_item",
            "mod_item",
            "const_item",
            "static_item",
            "type_item",
            "macro_definition",
        ]
    }
}

impl RustExtractor {
    /// Recursively extract semantic units from the AST.
    fn extract_from_node(
        &self,
        cursor: &mut TreeCursor,
        source: &[u8],
        units: &mut Vec<SemanticUnit>,
        parent_context: Option<&str>,
    ) {
        let node = cursor.node();

        // Try to extract a semantic unit from this node
        if let Some(unit) = self.extract_unit(&node, source, parent_context) {
            units.push(unit);
        }

        // For impl blocks, pass the impl target as context
        let new_context = if node.kind() == "impl_item" {
            self.get_impl_target(&node, source)
        } else {
            parent_context.map(|s| s.to_string())
        };

        // Recurse into children
        if cursor.goto_first_child() {
            loop {
                self.extract_from_node(cursor, source, units, new_context.as_deref());
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
        parent_context: Option<&str>,
    ) -> Option<SemanticUnit> {
        let kind = match node.kind() {
            "function_item" => {
                // Check if this is a test function
                if self.is_test_function(node, source) {
                    SemanticKind::Test
                } else if parent_context.is_some() {
                    SemanticKind::Method
                } else {
                    SemanticKind::Function
                }
            }
            "struct_item" => SemanticKind::Struct,
            "enum_item" => SemanticKind::Enum,
            "trait_item" => SemanticKind::Trait,
            "impl_item" => SemanticKind::Impl,
            "mod_item" => SemanticKind::Module,
            "const_item" | "static_item" => SemanticKind::Constant,
            "type_item" => SemanticKind::TypeAlias,
            "macro_definition" => SemanticKind::Macro,
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
            start_line: node.start_position().row + 1, // 1-indexed
            end_line: node.end_position().row + 1,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            signature,
            parent: parent_context.map(|s| s.to_string()),
        })
    }

    /// Get the name/identifier from a node.
    fn get_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        let name_node = match node.kind() {
            "function_item" => node.child_by_field_name("name"),
            "struct_item" | "enum_item" | "trait_item" | "type_item" => {
                node.child_by_field_name("name")
            }
            "impl_item" => {
                // For impl, return the impl target as the name
                return self.get_impl_target(node, source);
            }
            "mod_item" => node.child_by_field_name("name"),
            "const_item" | "static_item" => node.child_by_field_name("name"),
            "macro_definition" => node.child_by_field_name("name"),
            _ => None,
        };

        name_node.map(|n| node_text(&n, source).to_string())
    }

    /// Get the impl target (type name) for an impl block.
    fn get_impl_target(&self, node: &Node, source: &[u8]) -> Option<String> {
        // Look for the type being implemented
        // impl Trait for Type or impl Type
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "type_identifier" || child.kind() == "generic_type" {
                    return Some(node_text(&child, source).to_string());
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        None
    }

    /// Get function/method signature.
    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        if node.kind() != "function_item" {
            return None;
        }

        // Build signature from visibility, async, fn keyword, name, and parameters
        let mut parts = Vec::new();

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "visibility_modifier" | "function_modifiers" => {
                        parts.push(node_text(&child, source).to_string());
                    }
                    "identifier" => {
                        // This is the function name
                        parts.push(format!("fn {}", node_text(&child, source)));
                    }
                    "parameters" => {
                        parts.push(node_text(&child, source).to_string());
                    }
                    "type_parameters" => {
                        parts.push(node_text(&child, source).to_string());
                    }
                    "->" => {
                        // Start of return type
                        if cursor.goto_next_sibling() {
                            let return_type = cursor.node();
                            parts.push(format!("-> {}", node_text(&return_type, source)));
                        }
                        break;
                    }
                    "block" => break, // Stop at function body
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" ").trim().to_string())
        }
    }

    /// Get doc comments associated with a node.
    fn get_docs(&self, node: &Node, source: &[u8]) -> Option<String> {
        // Look for preceding doc comments (/// or //!)
        let mut docs = Vec::new();

        // Check for attribute nodes that might be doc comments
        if let Some(prev) = node.prev_sibling() {
            Self::collect_doc_comments(&prev, source, &mut docs);
        }

        // Also check for inner attributes on the node itself
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "line_comment" {
                    let text = node_text(&child, source);
                    if text.starts_with("///") || text.starts_with("//!") {
                        docs.push(text.to_string());
                    }
                } else if child.kind() == "attribute_item" {
                    // Check for #[doc = "..."]
                    let attr_text = node_text(&child, source);
                    if attr_text.contains("doc") {
                        docs.push(attr_text.to_string());
                    }
                } else if child.kind() != "line_comment" && child.kind() != "attribute_item" {
                    break; // Stop at first non-doc node
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }

    /// Collect doc comments from a node and its preceding siblings.
    fn collect_doc_comments(node: &Node, source: &[u8], docs: &mut Vec<String>) {
        if node.kind() == "line_comment" {
            let text = node_text(node, source);
            if text.starts_with("///") || text.starts_with("//!") {
                docs.insert(0, text.to_string());
            }
        } else if node.kind() == "attribute_item" {
            let text = node_text(node, source);
            if text.contains("doc") {
                docs.insert(0, text.to_string());
            }
        }

        // Recurse to previous siblings
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "line_comment" || prev.kind() == "attribute_item" {
                Self::collect_doc_comments(&prev, source, docs);
            }
        }
    }

    /// Check if a function has the #[test] attribute.
    fn is_test_function(&self, node: &Node, source: &[u8]) -> bool {
        // Check preceding siblings for #[test] attribute
        let mut prev = node.prev_sibling();
        while let Some(sibling) = prev {
            if sibling.kind() == "attribute_item" {
                let text = node_text(&sibling, source);
                if text.contains("test") || text.contains("tokio::test") || text.contains("rstest")
                {
                    return true;
                }
            } else if sibling.kind() != "line_comment" {
                break;
            }
            prev = sibling.prev_sibling();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_rust(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Failed to set Rust language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
fn hello_world() {
    println!("Hello, world!");
}
"#;
        let tree = parse_rust(source);
        let extractor = RustExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert_eq!(units[0].name, Some("hello_world".to_string()));
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
pub struct User {
    name: String,
    age: u32,
}
"#;
        let tree = parse_rust(source);
        let extractor = RustExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Struct);
        assert_eq!(units[0].name, Some("User".to_string()));
    }

    #[test]
    fn test_extract_impl_with_methods() {
        let source = r#"
impl User {
    pub fn new(name: String) -> Self {
        Self { name, age: 0 }
    }

    pub fn greet(&self) {
        println!("Hello, {}", self.name);
    }
}
"#;
        let tree = parse_rust(source);
        let extractor = RustExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        // Should extract: impl block, new method, greet method
        assert!(units.len() >= 3);

        let impl_unit = units.iter().find(|u| u.kind == SemanticKind::Impl);
        assert!(impl_unit.is_some());

        let methods: Vec<_> = units.iter().filter(|u| u.kind == SemanticKind::Method).collect();
        assert_eq!(methods.len(), 2);

        // Methods should have parent context
        for method in methods {
            assert!(method.parent.is_some());
        }
    }

    #[test]
    fn test_extract_trait() {
        let source = r#"
pub trait Greetable {
    fn greet(&self);
    fn farewell(&self) {
        println!("Goodbye!");
    }
}
"#;
        let tree = parse_rust(source);
        let extractor = RustExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let trait_unit = units.iter().find(|u| u.kind == SemanticKind::Trait);
        assert!(trait_unit.is_some());
        assert_eq!(trait_unit.unwrap().name, Some("Greetable".to_string()));
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
pub enum Status {
    Active,
    Inactive,
    Pending(String),
}
"#;
        let tree = parse_rust(source);
        let extractor = RustExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Enum);
        assert_eq!(units[0].name, Some("Status".to_string()));
    }

    #[test]
    fn test_extract_module() {
        let source = r#"
mod inner {
    pub fn helper() {}
}
"#;
        let tree = parse_rust(source);
        let extractor = RustExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let mod_unit = units.iter().find(|u| u.kind == SemanticKind::Module);
        assert!(mod_unit.is_some());
        assert_eq!(mod_unit.unwrap().name, Some("inner".to_string()));
    }

    #[test]
    fn test_line_numbers() {
        let source = r#"// Comment
fn first() {}

fn second() {}
"#;
        let tree = parse_rust(source);
        let extractor = RustExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 2);
        assert_eq!(units[0].start_line, 2); // fn first() is on line 2
        assert_eq!(units[1].start_line, 4); // fn second() is on line 4
    }
}
