//! Go-specific semantic extractor.
//!
//! Extracts: function_declaration, method_declaration, type_declaration (struct, interface)

use tree_sitter::{Node, Tree, TreeCursor};

use super::{node_text, SemanticExtractor, SemanticKind, SemanticUnit};

/// Go language semantic extractor.
pub struct GoExtractor;

impl SemanticExtractor for GoExtractor {
    fn language_id(&self) -> &'static str {
        "go"
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
            "function_declaration",
            "method_declaration",
            "type_declaration",
            "const_declaration",
            "var_declaration",
        ]
    }
}

impl GoExtractor {
    /// Recursively extract semantic units from the AST.
    fn extract_from_node(
        &self,
        cursor: &mut TreeCursor,
        source: &[u8],
        units: &mut Vec<SemanticUnit>,
    ) {
        let node = cursor.node();

        // Try to extract a semantic unit from this node
        if let Some(extracted) = self.extract_unit(&node, source) {
            // For type declarations, we might get multiple units
            for unit in extracted {
                units.push(unit);
            }
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

    /// Extract semantic units from a node if it's a target type.
    fn extract_unit(&self, node: &Node, source: &[u8]) -> Option<Vec<SemanticUnit>> {
        match node.kind() {
            "function_declaration" => {
                let name = self.get_function_name(node, source);
                let kind = if self.is_test_function(&name) {
                    SemanticKind::Test
                } else {
                    SemanticKind::Function
                };

                Some(vec![SemanticUnit {
                    kind,
                    name,
                    content: node_text(node, source).to_string(),
                    docs: self.get_doc_comment(node, source),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                    signature: self.get_function_signature(node, source),
                    parent: None,
                }])
            }
            "method_declaration" => {
                let name = self.get_method_name(node, source);
                let receiver = self.get_method_receiver(node, source);
                let kind = if self.is_test_function(&name) {
                    SemanticKind::Test
                } else {
                    SemanticKind::Method
                };

                Some(vec![SemanticUnit {
                    kind,
                    name,
                    content: node_text(node, source).to_string(),
                    docs: self.get_doc_comment(node, source),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                    signature: self.get_method_signature(node, source),
                    parent: receiver,
                }])
            }
            "type_declaration" => self.extract_type_declaration(node, source),
            "const_declaration" | "var_declaration" => {
                // Only extract if it defines significant values
                let content = node_text(node, source);
                if content.len() > 20 {
                    // Skip trivial declarations
                    Some(vec![SemanticUnit {
                        kind: SemanticKind::Constant,
                        name: self.get_const_name(node, source),
                        content: content.to_string(),
                        docs: self.get_doc_comment(node, source),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        signature: None,
                        parent: None,
                    }])
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Extract type declarations (struct, interface).
    fn extract_type_declaration(&self, node: &Node, source: &[u8]) -> Option<Vec<SemanticUnit>> {
        let mut units = Vec::new();

        // Type declaration can contain multiple type specs
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "type_spec" {
                    if let Some(unit) = self.extract_type_spec(&child, source) {
                        units.push(unit);
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if units.is_empty() {
            None
        } else {
            Some(units)
        }
    }

    /// Extract a single type spec (struct or interface).
    fn extract_type_spec(&self, node: &Node, source: &[u8]) -> Option<SemanticUnit> {
        let name = node.child_by_field_name("name").map(|n| node_text(&n, source).to_string());

        let type_node = node.child_by_field_name("type")?;
        let kind = match type_node.kind() {
            "struct_type" => SemanticKind::Struct,
            "interface_type" => SemanticKind::Interface,
            _ => SemanticKind::TypeAlias,
        };

        Some(SemanticUnit {
            kind,
            name,
            content: node_text(node, source).to_string(),
            docs: self.get_doc_comment(node, source),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            signature: None,
            parent: None,
        })
    }

    /// Get function name.
    fn get_function_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
    }

    /// Get method name.
    fn get_method_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
    }

    /// Get method receiver type.
    fn get_method_receiver(&self, node: &Node, source: &[u8]) -> Option<String> {
        let receiver = node.child_by_field_name("receiver")?;

        // Look for the type identifier in the receiver
        let mut cursor = receiver.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "parameter_declaration" {
                    // Find the type within the parameter
                    if let Some(type_node) = child.child_by_field_name("type") {
                        let type_text = node_text(&type_node, source);
                        // Remove pointer if present
                        return Some(type_text.trim_start_matches('*').to_string());
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        None
    }

    /// Get function signature.
    fn get_function_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut parts = vec!["func".to_string()];

        if let Some(name) = node.child_by_field_name("name") {
            parts.push(node_text(&name, source).to_string());
        }

        if let Some(params) = node.child_by_field_name("parameters") {
            parts.push(node_text(&params, source).to_string());
        }

        if let Some(result) = node.child_by_field_name("result") {
            parts.push(node_text(&result, source).to_string());
        }

        Some(parts.join(" "))
    }

    /// Get method signature.
    fn get_method_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut parts = vec!["func".to_string()];

        if let Some(receiver) = node.child_by_field_name("receiver") {
            parts.push(node_text(&receiver, source).to_string());
        }

        if let Some(name) = node.child_by_field_name("name") {
            parts.push(node_text(&name, source).to_string());
        }

        if let Some(params) = node.child_by_field_name("parameters") {
            parts.push(node_text(&params, source).to_string());
        }

        if let Some(result) = node.child_by_field_name("result") {
            parts.push(node_text(&result, source).to_string());
        }

        Some(parts.join(" "))
    }

    /// Get const/var name.
    fn get_const_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "const_spec" || child.kind() == "var_spec" {
                    if let Some(name) = child.child_by_field_name("name") {
                        return Some(node_text(&name, source).to_string());
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        None
    }

    /// Get doc comment (Go uses // comments before declarations).
    fn get_doc_comment(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut docs = Vec::new();
        let mut prev = node.prev_sibling();

        while let Some(sibling) = prev {
            if sibling.kind() == "comment" {
                let text = node_text(&sibling, source);
                docs.insert(0, text.to_string());
            } else {
                break;
            }
            prev = sibling.prev_sibling();
        }

        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }

    /// Check if a function name indicates a test.
    fn is_test_function(&self, name: &Option<String>) -> bool {
        name.as_ref()
            .map(|n| {
                n.starts_with("Test")
                    || n.starts_with("Benchmark")
                    || n.starts_with("Example")
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_go(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("Failed to set Go language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
package main

func hello() {
    fmt.Println("Hello")
}
"#;
        let tree = parse_go(source);
        let extractor = GoExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert_eq!(units[0].name, Some("hello".to_string()));
    }

    #[test]
    fn test_extract_method() {
        let source = r#"
package main

type User struct {
    Name string
}

func (u *User) Greet() string {
    return "Hello, " + u.Name
}
"#;
        let tree = parse_go(source);
        let extractor = GoExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        // Should extract: struct and method
        assert!(units.len() >= 2);

        let method = units.iter().find(|u| u.kind == SemanticKind::Method);
        assert!(method.is_some());
        assert_eq!(method.unwrap().name, Some("Greet".to_string()));
        assert_eq!(method.unwrap().parent, Some("User".to_string()));
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
package main

type User struct {
    Name string
    Age  int
}
"#;
        let tree = parse_go(source);
        let extractor = GoExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Struct);
        assert_eq!(units[0].name, Some("User".to_string()));
    }

    #[test]
    fn test_extract_interface() {
        let source = r#"
package main

type Reader interface {
    Read(p []byte) (n int, err error)
}
"#;
        let tree = parse_go(source);
        let extractor = GoExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Interface);
        assert_eq!(units[0].name, Some("Reader".to_string()));
    }

    #[test]
    fn test_extract_test_function() {
        let source = r#"
package main

func TestSomething(t *testing.T) {
    // test code
}
"#;
        let tree = parse_go(source);
        let extractor = GoExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Test);
    }

    #[test]
    fn test_function_signature() {
        let source = r#"
package main

func greet(name string) string {
    return "Hello, " + name
}
"#;
        let tree = parse_go(source);
        let extractor = GoExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        let sig = units[0].signature.as_ref().unwrap();
        assert!(sig.contains("func"));
        assert!(sig.contains("greet"));
        assert!(sig.contains("name string"));
        assert!(sig.contains("string"));
    }
}
