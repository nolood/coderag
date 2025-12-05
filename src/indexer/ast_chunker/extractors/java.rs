//! Java-specific semantic extractor.
//!
//! Extracts: method_declaration, class_declaration, interface_declaration,
//! enum_declaration, constructor_declaration

use tree_sitter::{Node, Tree, TreeCursor};

use super::{node_text, SemanticExtractor, SemanticKind, SemanticUnit};

/// Java language semantic extractor.
pub struct JavaExtractor;

impl SemanticExtractor for JavaExtractor {
    fn language_id(&self) -> &'static str {
        "java"
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
            "class_declaration",
            "interface_declaration",
            "enum_declaration",
            "method_declaration",
            "constructor_declaration",
            "field_declaration",
        ]
    }
}

impl JavaExtractor {
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

        // For class/interface definitions, pass the name as context
        let new_context = match node.kind() {
            "class_declaration" | "interface_declaration" | "enum_declaration" => {
                self.get_name(&node, source)
            }
            _ => parent_context.map(|s| s.to_string()),
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
            "class_declaration" => SemanticKind::Class,
            "interface_declaration" => SemanticKind::Interface,
            "enum_declaration" => SemanticKind::Enum,
            "method_declaration" => {
                let name = self.get_name(node, source);
                if self.is_test_method(node, source) || self.is_test_name(&name) {
                    SemanticKind::Test
                } else {
                    SemanticKind::Method
                }
            }
            "constructor_declaration" => SemanticKind::Method,
            "field_declaration" => {
                // Only extract if it's a constant (static final)
                if self.is_constant_field(node, source) {
                    SemanticKind::Constant
                } else {
                    return None;
                }
            }
            _ => return None,
        };

        let name = self.get_name(node, source);
        let signature = self.get_signature(node, source);
        let docs = self.get_javadoc(node, source);
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

    /// Get method/constructor signature.
    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "method_declaration" => {
                let mut parts = Vec::new();

                // Get modifiers
                if let Some(modifiers) = self.get_modifiers(node, source) {
                    parts.push(modifiers);
                }

                // Get return type
                if let Some(return_type) = node.child_by_field_name("type") {
                    parts.push(node_text(&return_type, source).to_string());
                }

                // Get name
                if let Some(name) = node.child_by_field_name("name") {
                    parts.push(node_text(&name, source).to_string());
                }

                // Get parameters
                if let Some(params) = node.child_by_field_name("parameters") {
                    parts.push(node_text(&params, source).to_string());
                }

                // Get throws clause
                if let Some(throws) = self.get_throws_clause(node, source) {
                    parts.push(format!("throws {}", throws));
                }

                Some(parts.join(" "))
            }
            "constructor_declaration" => {
                let mut parts = Vec::new();

                // Get modifiers
                if let Some(modifiers) = self.get_modifiers(node, source) {
                    parts.push(modifiers);
                }

                // Get name
                if let Some(name) = node.child_by_field_name("name") {
                    parts.push(node_text(&name, source).to_string());
                }

                // Get parameters
                if let Some(params) = node.child_by_field_name("parameters") {
                    parts.push(node_text(&params, source).to_string());
                }

                Some(parts.join(" "))
            }
            _ => None,
        }
    }

    /// Get modifiers (public, private, static, etc.).
    fn get_modifiers(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        let mut modifiers = Vec::new();

        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "modifiers" {
                    modifiers.push(node_text(&child, source).to_string());
                    break;
                }
                if child.kind() != "modifiers"
                    && child.kind() != "marker_annotation"
                    && child.kind() != "annotation"
                {
                    break;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if modifiers.is_empty() {
            None
        } else {
            Some(modifiers.join(" "))
        }
    }

    /// Get throws clause.
    fn get_throws_clause(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "throws" {
                    // Get the exception types
                    let mut throws = Vec::new();
                    let mut inner_cursor = child.walk();
                    if inner_cursor.goto_first_child() {
                        loop {
                            let inner_child = inner_cursor.node();
                            if inner_child.kind() == "type_identifier" {
                                throws.push(node_text(&inner_child, source).to_string());
                            }
                            if !inner_cursor.goto_next_sibling() {
                                break;
                            }
                        }
                    }
                    if !throws.is_empty() {
                        return Some(throws.join(", "));
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        None
    }

    /// Get JavaDoc comment.
    fn get_javadoc(&self, node: &Node, source: &[u8]) -> Option<String> {
        // Look for preceding block comment that starts with /**
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "block_comment" || prev.kind() == "comment" {
                let text = node_text(&prev, source);
                if text.starts_with("/**") {
                    return Some(text.to_string());
                }
            }
        }
        None
    }

    /// Check if field is a constant (static final).
    fn is_constant_field(&self, node: &Node, source: &[u8]) -> bool {
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "modifiers" {
                    let text = node_text(&child, source);
                    return text.contains("static") && text.contains("final");
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        false
    }

    /// Check if method has test annotation.
    fn is_test_method(&self, node: &Node, source: &[u8]) -> bool {
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "marker_annotation" || child.kind() == "annotation" {
                    let text = node_text(&child, source);
                    if text.contains("Test")
                        || text.contains("ParameterizedTest")
                        || text.contains("RepeatedTest")
                    {
                        return true;
                    }
                }
                // Stop at modifiers or return type
                if child.kind() == "modifiers"
                    || child.kind() == "type_identifier"
                    || child.kind() == "void_type"
                {
                    break;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        false
    }

    /// Check if name indicates a test.
    fn is_test_name(&self, name: &Option<String>) -> bool {
        name.as_ref()
            .map(|n| n.starts_with("test") || n.starts_with("Test"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_java(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("Failed to set Java language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
public class User {
    private String name;

    public User(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }
}
"#;
        let tree = parse_java(source);
        let extractor = JavaExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        // Should extract: class, constructor, method
        assert!(units.len() >= 3);

        let class_unit = units.iter().find(|u| u.kind == SemanticKind::Class);
        assert!(class_unit.is_some());
        assert_eq!(class_unit.unwrap().name, Some("User".to_string()));

        let methods: Vec<_> = units.iter().filter(|u| u.kind == SemanticKind::Method).collect();
        assert_eq!(methods.len(), 2); // constructor + getName
    }

    #[test]
    fn test_extract_interface() {
        let source = r#"
public interface Greeter {
    String greet(String name);
}
"#;
        let tree = parse_java(source);
        let extractor = JavaExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let interface = units.iter().find(|u| u.kind == SemanticKind::Interface);
        assert!(interface.is_some());
        assert_eq!(interface.unwrap().name, Some("Greeter".to_string()));
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
public enum Status {
    ACTIVE,
    INACTIVE,
    PENDING
}
"#;
        let tree = parse_java(source);
        let extractor = JavaExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let enum_unit = units.iter().find(|u| u.kind == SemanticKind::Enum);
        assert!(enum_unit.is_some());
        assert_eq!(enum_unit.unwrap().name, Some("Status".to_string()));
    }

    #[test]
    fn test_extract_test_method() {
        let source = r#"
public class UserTest {
    @Test
    public void testUserCreation() {
        // test code
    }
}
"#;
        let tree = parse_java(source);
        let extractor = JavaExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let test = units.iter().find(|u| u.kind == SemanticKind::Test);
        assert!(test.is_some());
        assert_eq!(test.unwrap().name, Some("testUserCreation".to_string()));
    }

    #[test]
    fn test_method_signature() {
        let source = r#"
public class Example {
    public String greet(String name) throws IOException {
        return "Hello, " + name;
    }
}
"#;
        let tree = parse_java(source);
        let extractor = JavaExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let method = units.iter().find(|u| u.kind == SemanticKind::Method);
        assert!(method.is_some());

        let sig = method.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("public"));
        assert!(sig.contains("String"));
        assert!(sig.contains("greet"));
    }

    #[test]
    fn test_parent_context() {
        let source = r#"
public class Outer {
    public void innerMethod() {}
}
"#;
        let tree = parse_java(source);
        let extractor = JavaExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let method = units.iter().find(|u| u.kind == SemanticKind::Method);
        assert!(method.is_some());
        assert_eq!(method.unwrap().parent, Some("Outer".to_string()));
    }

    #[test]
    fn test_extract_constant() {
        let source = r#"
public class Constants {
    public static final String API_KEY = "secret";
}
"#;
        let tree = parse_java(source);
        let extractor = JavaExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        let constant = units.iter().find(|u| u.kind == SemanticKind::Constant);
        assert!(constant.is_some());
    }
}
