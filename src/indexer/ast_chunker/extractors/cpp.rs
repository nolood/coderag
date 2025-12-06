//! C++-specific semantic extractor.
//!
//! Extracts: function_definition, class_specifier, struct_specifier,
//! enum_specifier, template_declaration, namespace_definition

use tree_sitter::{Node, Tree, TreeCursor};

use super::{node_text, SemanticExtractor, SemanticKind, SemanticUnit};

/// C++ language semantic extractor.
pub struct CppExtractor;

impl SemanticExtractor for CppExtractor {
    fn language_id(&self) -> &'static str {
        "cpp"
    }

    fn extract(&self, tree: &Tree, source: &[u8]) -> Vec<SemanticUnit> {
        let mut units = Vec::new();
        let mut cursor = tree.walk();

        self.extract_from_node(&mut cursor, source, &mut units, None);

        units.sort_by_key(|u| (u.start_line, u.start_byte));

        units
    }

    fn target_node_types(&self) -> &[&'static str] {
        &[
            "function_definition",
            "class_specifier",
            "struct_specifier",
            "enum_specifier",
            "template_declaration",
            "namespace_definition",
            "type_definition",
            "alias_declaration",  // C++ using alias
        ]
    }
}

impl CppExtractor {
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

        // Track namespace context for nested declarations
        let new_context = if node.kind() == "namespace_definition" {
            self.get_namespace_name(&node, source)
        } else if node.kind() == "class_specifier" {
            self.get_class_name(&node, source)
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
                // Check if it's a method (inside a class)
                if parent_context.is_some() {
                    SemanticKind::Method
                } else {
                    SemanticKind::Function
                }
            }
            "class_specifier" => SemanticKind::Class,
            "struct_specifier" => SemanticKind::Struct,
            "enum_specifier" | "enum_class" => SemanticKind::Enum,
            "template_declaration" => {
                // Look at the child to determine what kind of template
                if let Some(child) = node.child(1) {
                    match child.kind() {
                        "class_specifier" => SemanticKind::Class,
                        "struct_specifier" => SemanticKind::Struct,
                        "function_definition" => SemanticKind::Function,
                        _ => SemanticKind::Block,
                    }
                } else {
                    SemanticKind::Block
                }
            }
            "namespace_definition" => SemanticKind::Module,
            "type_definition" | "alias_declaration" => SemanticKind::TypeAlias,
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
            parent: parent_context.map(|s| s.to_string()),
        })
    }

    fn get_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_definition" => {
                // Find identifier in function_declarator
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child_node = cursor.node();
                        if child_node.kind() == "function_declarator" {
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
            "class_specifier" | "struct_specifier" => {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "enum_specifier" | "enum_class" => {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "namespace_definition" => {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "template_declaration" => {
                // For templates, get the name of the templated entity
                if let Some(child) = node.child(1) {
                    self.get_name(&child, source)
                } else {
                    None
                }
            }
            "alias_declaration" => {
                // Using alias: using NewName = OldType;
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "type_definition" => {
                // typedef: similar to C
                node.child_by_field_name("declarator")
                    .and_then(|d| self.find_identifier_in_declarator(&d, source))
            }
            _ => None,
        }
    }

    /// Find identifier within a declarator node (handles nested declarators)
    fn find_identifier_in_declarator(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "identifier" | "field_identifier" | "destructor_name" => {
                Some(node_text(node, source).to_string())
            }
            "qualified_identifier" => {
                // Qualified names like std::vector
                Some(node_text(node, source).to_string())
            }
            "function_declarator" | "pointer_declarator" | "reference_declarator" | "array_declarator" => {
                // These contain the identifier as a child
                node.child_by_field_name("declarator")
                    .and_then(|d| self.find_identifier_in_declarator(&d, source))
            }
            _ => {
                // Try to find an identifier child directly
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child_kind = cursor.node().kind();
                        if child_kind == "identifier" || child_kind == "field_identifier" || child_kind == "qualified_identifier" {
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

    fn get_namespace_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
    }

    fn get_class_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
    }

    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_definition" => {
                // Extract return type, name, and parameters
                let mut parts = Vec::new();
                let mut found_declarator = false;

                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child = cursor.node();
                        match child.kind() {
                            "primitive_type" | "type_identifier" | "qualified_identifier" | "sized_type_specifier" => {
                                if !found_declarator {
                                    parts.push(node_text(&child, source).to_string());
                                }
                            }
                            "template_type" => {
                                if !found_declarator {
                                    parts.push(node_text(&child, source).to_string());
                                }
                            }
                            "function_declarator" => {
                                parts.push(node_text(&child, source).to_string());
                                found_declarator = true;
                            }
                            "virtual" | "explicit" | "inline" | "static" | "constexpr" | "consteval" => {
                                parts.insert(0, node_text(&child, source).to_string());
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
            "template_declaration" => {
                // For templates, include template parameters
                let mut template_params = String::new();
                let mut cursor = node.walk();
                if cursor.goto_first_child()
                    && cursor.node().kind() == "template_parameter_list" {
                        template_params = format!("template{}", node_text(&cursor.node(), source));
                }

                // Get signature of the templated entity
                if let Some(child) = node.child(1) {
                    if let Some(sig) = self.get_signature(&child, source) {
                        Some(format!("{} {}", template_params, sig))
                    } else {
                        Some(template_params)
                    }
                } else {
                    Some(template_params)
                }
            }
            "class_specifier" => {
                // Include inheritance if present
                let mut sig = String::from("class");
                if let Some(name) = self.get_name(node, source) {
                    sig.push(' ');
                    sig.push_str(&name);
                }

                // Check for base classes
                if let Some(base_clause) = node.child_by_field_name("superclass") {
                    sig.push_str(" : ");
                    sig.push_str(node_text(&base_clause, source));
                }

                Some(sig)
            }
            _ => None,
        }
    }

    fn get_docs(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut docs = Vec::new();

        // Check previous siblings for comments
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
        if trimmed.starts_with("///") {
            // Doxygen-style comment
            trimmed.trim_start_matches("///").trim().to_string()
        } else if trimmed.starts_with("//!") {
            // Doxygen-style comment
            trimmed.trim_start_matches("//!").trim().to_string()
        } else if trimmed.starts_with("//") {
            trimmed.trim_start_matches("//").trim().to_string()
        } else if trimmed.starts_with("/**") && trimmed.ends_with("*/") {
            // Doxygen block comment
            trimmed
                .trim_start_matches("/**")
                .trim_end_matches("*/")
                .lines()
                .map(|line| line.trim().trim_start_matches('*').trim())
                .collect::<Vec<_>>()
                .join("\n")
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

    fn parse_cpp(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("Failed to set C++ language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
class Point {
public:
    int x;
    int y;

    Point(int x, int y) : x(x), y(y) {}
};
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.iter().any(|u| u.kind == SemanticKind::Class && u.name == Some("Point".to_string())));
        // Constructor should also be found as a method
        assert!(units.iter().any(|u| u.kind == SemanticKind::Method));
    }

    #[test]
    fn test_extract_template() {
        let source = r#"
template <typename T>
class Container {
    T* data;
public:
    T get(int index) { return data[index]; }
};
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        // Should find template class
        assert!(units.iter().any(|u| u.kind == SemanticKind::Class && u.name == Some("Container".to_string())));
    }

    #[test]
    fn test_extract_namespace() {
        let source = r#"
namespace utils {
    int helper() { return 42; }
}
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.iter().any(|u| u.kind == SemanticKind::Module && u.name == Some("utils".to_string())));
        assert!(units.iter().any(|u| u.kind == SemanticKind::Function && u.name == Some("helper".to_string())));
    }

    #[test]
    fn test_extract_method() {
        let source = r#"
class Calculator {
public:
    int add(int a, int b) {
        return a + b;
    }

    virtual int multiply(int a, int b) = 0;
};
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        // Should find class
        assert!(units.iter().any(|u| u.kind == SemanticKind::Class));
        // Should find methods
        assert!(units.iter().any(|u| u.kind == SemanticKind::Method && u.name == Some("add".to_string())));
    }

    #[test]
    fn test_extract_enum_class() {
        let source = r#"
enum class Color : uint8_t {
    RED = 0,
    GREEN = 1,
    BLUE = 2
};
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.iter().any(|u| u.kind == SemanticKind::Enum && u.name == Some("Color".to_string())));
    }

    #[test]
    fn test_extract_using_alias() {
        let source = r#"
using String = std::string;
using IntVector = std::vector<int>;
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.iter().any(|u| u.kind == SemanticKind::TypeAlias && u.name == Some("String".to_string())));
        assert!(units.iter().any(|u| u.kind == SemanticKind::TypeAlias && u.name == Some("IntVector".to_string())));
    }

    #[test]
    fn test_extract_template_function() {
        let source = r#"
template<typename T>
T max(T a, T b) {
    return (a > b) ? a : b;
}
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.iter().any(|u| u.kind == SemanticKind::Function && u.name == Some("max".to_string())));
    }

    #[test]
    fn test_extract_with_inheritance() {
        let source = r#"
class Shape {
public:
    virtual double area() = 0;
};

class Circle : public Shape {
    double radius;
public:
    double area() override {
        return 3.14159 * radius * radius;
    }
};
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.iter().any(|u| u.kind == SemanticKind::Class && u.name == Some("Shape".to_string())));
        assert!(units.iter().any(|u| u.kind == SemanticKind::Class && u.name == Some("Circle".to_string())));
    }

    #[test]
    fn test_extract_struct() {
        let source = r#"
struct Vector3D {
    float x, y, z;

    float magnitude() const {
        return sqrt(x*x + y*y + z*z);
    }
};
"#;
        let tree = parse_cpp(source);
        let extractor = CppExtractor;
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.iter().any(|u| u.kind == SemanticKind::Struct && u.name == Some("Vector3D".to_string())));
        assert!(units.iter().any(|u| u.kind == SemanticKind::Method && u.name == Some("magnitude".to_string())));
    }
}