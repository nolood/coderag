//! Python-specific semantic extractor.
//!
//! Extracts: function_definition, class_definition, decorated functions/classes

use tree_sitter::{Node, Tree, TreeCursor};

use super::{node_text, SemanticExtractor, SemanticKind, SemanticUnit};

/// Python language semantic extractor.
pub struct PythonExtractor;

impl SemanticExtractor for PythonExtractor {
    fn language_id(&self) -> &'static str {
        "python"
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
            "function_definition",
            "class_definition",
            "decorated_definition",
        ]
    }
}

impl PythonExtractor {
    /// Recursively extract semantic units from the AST.
    fn extract_from_node(
        &self,
        cursor: &mut TreeCursor,
        source: &[u8],
        units: &mut Vec<SemanticUnit>,
        parent_context: Option<&str>,
    ) {
        let node = cursor.node();

        // Handle decorated definitions specially
        if node.kind() == "decorated_definition" {
            if let Some(unit) = self.extract_decorated(&node, source, parent_context) {
                units.push(unit);
            }
            // Don't recurse into decorated_definition, we've handled it
            return;
        }

        // Try to extract a semantic unit from this node
        if let Some(unit) = self.extract_unit(&node, source, parent_context) {
            units.push(unit);
        }

        // For class definitions, pass the class name as context
        let new_context = if node.kind() == "class_definition" {
            self.get_name(&node, source)
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
            "function_definition" => {
                let name = self.get_name(node, source);
                // Check if this is a test function or a method
                if name
                    .as_ref()
                    .map(|n| n.starts_with("test_") || n.starts_with("test"))
                    .unwrap_or(false)
                {
                    SemanticKind::Test
                } else if parent_context.is_some() {
                    SemanticKind::Method
                } else {
                    SemanticKind::Function
                }
            }
            "class_definition" => SemanticKind::Class,
            _ => return None,
        };

        let name = self.get_name(node, source);
        let signature = self.get_signature(node, source);
        let docs = self.get_docstring(node, source);
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

    /// Extract a decorated definition (function or class with decorators).
    fn extract_decorated(
        &self,
        node: &Node,
        source: &[u8],
        parent_context: Option<&str>,
    ) -> Option<SemanticUnit> {
        // Find the actual definition inside the decorated_definition
        let mut cursor = node.walk();
        let mut decorators = Vec::new();
        let mut definition = None;

        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "decorator" {
                    decorators.push(node_text(&child, source).to_string());
                } else if child.kind() == "function_definition" || child.kind() == "class_definition"
                {
                    definition = Some(child);
                    break;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        let def_node = definition?;
        let name = self.get_name(&def_node, source);

        // Determine kind based on decorators and definition type
        let kind = if def_node.kind() == "class_definition" {
            SemanticKind::Class
        } else {
            // Check for test decorators
            let is_test = decorators.iter().any(|d| {
                d.contains("pytest")
                    || d.contains("test")
                    || d.contains("fixture")
                    || name
                        .as_ref()
                        .map(|n| n.starts_with("test_"))
                        .unwrap_or(false)
            });

            if is_test {
                SemanticKind::Test
            } else if parent_context.is_some() {
                SemanticKind::Method
            } else {
                SemanticKind::Function
            }
        };

        let signature = self.get_signature(&def_node, source);
        let docs = self.get_docstring(&def_node, source);

        // Content includes decorators
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
            parent: parent_context.map(|s| s.to_string()),
        })
    }

    /// Get the name/identifier from a node.
    fn get_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
    }

    /// Get function signature (def name(params) -> return_type).
    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        if node.kind() != "function_definition" {
            return None;
        }

        let mut parts = Vec::new();

        // Check for async
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "async" {
                    parts.push("async".to_string());
                    break;
                }
                if child.kind() == "def" {
                    break;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        parts.push("def".to_string());

        if let Some(name) = node.child_by_field_name("name") {
            parts.push(node_text(&name, source).to_string());
        }

        if let Some(params) = node.child_by_field_name("parameters") {
            parts.push(node_text(&params, source).to_string());
        }

        if let Some(return_type) = node.child_by_field_name("return_type") {
            parts.push(format!("-> {}", node_text(&return_type, source)));
        }

        if parts.len() <= 1 {
            None
        } else {
            Some(parts.join(" "))
        }
    }

    /// Get docstring from a function or class.
    fn get_docstring(&self, node: &Node, source: &[u8]) -> Option<String> {
        // Look for expression_statement containing a string as first child of body
        let body = node.child_by_field_name("body")?;

        let mut cursor = body.walk();
        if cursor.goto_first_child() {
            let first_child = cursor.node();
            if first_child.kind() == "expression_statement" {
                let mut inner_cursor = first_child.walk();
                if inner_cursor.goto_first_child() {
                    let expr = inner_cursor.node();
                    if expr.kind() == "string" {
                        return Some(node_text(&expr, source).to_string());
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_python(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("Failed to set Python language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
def hello_world():
    print("Hello, world!")
"#;
        let tree = parse_python(source);
        let extractor = PythonExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert_eq!(units[0].name, Some("hello_world".to_string()));
    }

    #[test]
    fn test_extract_async_function() {
        let source = r#"
async def fetch_data():
    await some_async_call()
"#;
        let tree = parse_python(source);
        let extractor = PythonExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert!(units[0].signature.as_ref().unwrap().contains("async"));
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
class User:
    def __init__(self, name):
        self.name = name

    def greet(self):
        print(f"Hello, {self.name}")
"#;
        let tree = parse_python(source);
        let extractor = PythonExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        // Should extract: class, __init__ method, greet method
        assert!(units.len() >= 3);

        let class_unit = units.iter().find(|u| u.kind == SemanticKind::Class);
        assert!(class_unit.is_some());
        assert_eq!(class_unit.unwrap().name, Some("User".to_string()));

        let methods: Vec<_> = units.iter().filter(|u| u.kind == SemanticKind::Method).collect();
        assert_eq!(methods.len(), 2);

        // Methods should have parent context
        for method in methods {
            assert_eq!(method.parent, Some("User".to_string()));
        }
    }

    #[test]
    fn test_extract_test_function() {
        let source = r#"
def test_something():
    assert True
"#;
        let tree = parse_python(source);
        let extractor = PythonExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Test);
    }

    #[test]
    fn test_extract_decorated_function() {
        let source = r#"
@pytest.fixture
def my_fixture():
    return 42
"#;
        let tree = parse_python(source);
        let extractor = PythonExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        // Fixture is treated as test
        assert_eq!(units[0].kind, SemanticKind::Test);
        assert!(units[0].content.contains("@pytest.fixture"));
    }

    #[test]
    fn test_extract_docstring() {
        let source = r#"
def documented():
    """This is a docstring."""
    pass
"#;
        let tree = parse_python(source);
        let extractor = PythonExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert!(units[0].docs.is_some());
        assert!(units[0].docs.as_ref().unwrap().contains("docstring"));
    }

    #[test]
    fn test_function_with_type_hints() {
        let source = r#"
def greet(name: str) -> str:
    return f"Hello, {name}"
"#;
        let tree = parse_python(source);
        let extractor = PythonExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        let sig = units[0].signature.as_ref().unwrap();
        assert!(sig.contains("name: str"));
        assert!(sig.contains("-> str"));
    }
}
