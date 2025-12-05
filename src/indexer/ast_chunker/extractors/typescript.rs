//! TypeScript/JavaScript semantic extractor.
//!
//! Extracts: function_declaration, class_declaration, method_definition,
//! interface_declaration, type_alias_declaration, arrow functions, etc.

use tree_sitter::{Node, Tree, TreeCursor};

use super::{node_text, SemanticExtractor, SemanticKind, SemanticUnit};

/// TypeScript/JavaScript language semantic extractor.
pub struct TypeScriptExtractor {
    /// Whether this extractor is for JavaScript (true) or TypeScript (false)
    is_javascript: bool,
}

impl TypeScriptExtractor {
    /// Create a new TypeScript/JavaScript extractor.
    ///
    /// # Arguments
    /// * `is_javascript` - If true, identifies as JavaScript, otherwise TypeScript
    pub fn new(is_javascript: bool) -> Self {
        Self { is_javascript }
    }
}

impl SemanticExtractor for TypeScriptExtractor {
    fn language_id(&self) -> &'static str {
        if self.is_javascript {
            "javascript"
        } else {
            "typescript"
        }
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
            "function_declaration",
            "function",
            "arrow_function",
            "class_declaration",
            "method_definition",
            "interface_declaration",
            "type_alias_declaration",
            "enum_declaration",
            "lexical_declaration", // for const arrow functions
            "export_statement",
        ]
    }
}

impl TypeScriptExtractor {
    /// Recursively extract semantic units from the AST.
    fn extract_from_node(
        &self,
        cursor: &mut TreeCursor,
        source: &[u8],
        units: &mut Vec<SemanticUnit>,
        parent_context: Option<&str>,
    ) {
        let node = cursor.node();

        // Handle export statements specially
        if node.kind() == "export_statement" {
            if let Some(unit) = self.extract_export(&node, source, parent_context) {
                units.push(unit);
            }
            // Don't recurse, we've handled it
            return;
        }

        // Handle lexical declarations (const/let with arrow functions)
        if node.kind() == "lexical_declaration" {
            if let Some(unit) = self.extract_lexical_declaration(&node, source, parent_context) {
                units.push(unit);
            }
            return;
        }

        // Try to extract a semantic unit from this node
        let extracted = self.extract_unit(&node, source, parent_context);
        if let Some(unit) = extracted {
            units.push(unit);
            // Don't recurse into function bodies - we've already captured them
            if matches!(
                node.kind(),
                "function_declaration" | "function" | "method_definition"
            ) {
                return;
            }
        }

        // For class definitions, pass the class name as context
        let new_context = if node.kind() == "class_declaration" || node.kind() == "class" {
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
            "function_declaration" | "function" => {
                let name = self.get_name(node, source);
                if self.is_test_function(&name) {
                    SemanticKind::Test
                } else {
                    SemanticKind::Function
                }
            }
            "arrow_function" => {
                // Arrow functions without a name are usually lambdas, skip them
                // They'll be captured as part of their parent
                return None;
            }
            "class_declaration" | "class" => SemanticKind::Class,
            "method_definition" => {
                let name = self.get_name(node, source);
                if self.is_test_function(&name) {
                    SemanticKind::Test
                } else {
                    SemanticKind::Method
                }
            }
            "interface_declaration" => SemanticKind::Interface,
            "type_alias_declaration" => SemanticKind::TypeAlias,
            "enum_declaration" => SemanticKind::Enum,
            _ => return None,
        };

        let name = self.get_name(node, source);
        let signature = self.get_signature(node, source);
        let docs = self.get_jsdoc(node, source);
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

    /// Extract from export statements.
    fn extract_export(
        &self,
        node: &Node,
        source: &[u8],
        parent_context: Option<&str>,
    ) -> Option<SemanticUnit> {
        // Find what's being exported
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "function_declaration" | "class_declaration" | "interface_declaration"
                    | "type_alias_declaration" | "enum_declaration" => {
                        // Extract the inner declaration but keep the export in content
                        let mut unit = self.extract_unit(&child, source, parent_context)?;
                        unit.content = node_text(node, source).to_string();
                        unit.start_line = node.start_position().row + 1;
                        unit.end_line = node.end_position().row + 1;
                        unit.start_byte = node.start_byte();
                        unit.end_byte = node.end_byte();
                        return Some(unit);
                    }
                    "lexical_declaration" => {
                        let mut unit =
                            self.extract_lexical_declaration(&child, source, parent_context)?;
                        unit.content = node_text(node, source).to_string();
                        unit.start_line = node.start_position().row + 1;
                        unit.start_byte = node.start_byte();
                        return Some(unit);
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        None
    }

    /// Extract from lexical declarations (const/let with arrow functions).
    fn extract_lexical_declaration(
        &self,
        node: &Node,
        source: &[u8],
        parent_context: Option<&str>,
    ) -> Option<SemanticUnit> {
        // Look for: const name = () => {} or const name = function() {}
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                if child.kind() == "variable_declarator" {
                    // Check if the value is an arrow function or function expression
                    let name = child.child_by_field_name("name");
                    let value = child.child_by_field_name("value");

                    if let (Some(name_node), Some(value_node)) = (name, value) {
                        if value_node.kind() == "arrow_function"
                            || value_node.kind() == "function"
                        {
                            let func_name = node_text(&name_node, source).to_string();
                            let kind = if self.is_test_function(&Some(func_name.clone())) {
                                SemanticKind::Test
                            } else if parent_context.is_some() {
                                SemanticKind::Method
                            } else {
                                SemanticKind::Function
                            };

                            return Some(SemanticUnit {
                                kind,
                                name: Some(func_name),
                                content: node_text(node, source).to_string(),
                                docs: self.get_jsdoc(node, source),
                                start_line: node.start_position().row + 1,
                                end_line: node.end_position().row + 1,
                                start_byte: node.start_byte(),
                                end_byte: node.end_byte(),
                                signature: self.get_arrow_signature(&name_node, &value_node, source),
                                parent: parent_context.map(|s| s.to_string()),
                            });
                        }
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        None
    }

    /// Get the name/identifier from a node.
    fn get_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_declaration" | "function" | "class_declaration" | "class"
            | "interface_declaration" | "type_alias_declaration" | "enum_declaration" => {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "method_definition" => node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source).to_string()),
            _ => None,
        }
    }

    /// Get function/method signature.
    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_declaration" | "function" | "method_definition" => {
                let mut parts = Vec::new();

                // Check for async
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child = cursor.node();
                        if child.kind() == "async" {
                            parts.push("async".to_string());
                        }
                        if child.kind() == "function" || child.kind() == "identifier" {
                            break;
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }

                parts.push("function".to_string());

                if let Some(name) = node.child_by_field_name("name") {
                    parts.push(node_text(&name, source).to_string());
                }

                if let Some(params) = node.child_by_field_name("parameters") {
                    parts.push(node_text(&params, source).to_string());
                }

                // TypeScript return type
                if let Some(return_type) = node.child_by_field_name("return_type") {
                    parts.push(format!(": {}", node_text(&return_type, source)));
                }

                if parts.len() <= 1 {
                    None
                } else {
                    Some(parts.join(" "))
                }
            }
            _ => None,
        }
    }

    /// Get signature for arrow function.
    fn get_arrow_signature(
        &self,
        name_node: &Node,
        value_node: &Node,
        source: &[u8],
    ) -> Option<String> {
        let name = node_text(name_node, source);

        // Get parameters from arrow function
        let params = value_node
            .child_by_field_name("parameters")
            .or_else(|| value_node.child_by_field_name("parameter"))
            .map(|p| node_text(&p, source).to_string())
            .unwrap_or_else(|| "()".to_string());

        // Get return type if present
        let return_type = value_node
            .child_by_field_name("return_type")
            .map(|r| format!(": {}", node_text(&r, source)))
            .unwrap_or_default();

        Some(format!("const {} = {}{} =>", name, params, return_type))
    }

    /// Get JSDoc comments associated with a node.
    fn get_jsdoc(&self, node: &Node, source: &[u8]) -> Option<String> {
        // Look for preceding comment node
        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "comment" {
                let text = node_text(&prev, source);
                if text.starts_with("/**") {
                    return Some(text.to_string());
                }
            }
        }
        None
    }

    /// Check if a function name indicates a test.
    fn is_test_function(&self, name: &Option<String>) -> bool {
        name.as_ref()
            .map(|n| {
                n.starts_with("test")
                    || n.starts_with("Test")
                    || n.starts_with("it")
                    || n.starts_with("describe")
                    || n.starts_with("should")
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_typescript(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Failed to set TypeScript language");
        parser.parse(source, None).expect("Failed to parse")
    }

    fn parse_javascript(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .expect("Failed to set JavaScript language");
        parser.parse(source, None).expect("Failed to parse")
    }

    #[test]
    fn test_extract_function() {
        let source = r#"
function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert_eq!(units[0].name, Some("greet".to_string()));
    }

    #[test]
    fn test_extract_class() {
        let source = r#"
class User {
    constructor(public name: string) {}

    greet(): string {
        return `Hello, ${this.name}`;
    }
}
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert!(units.len() >= 2);

        let class_unit = units.iter().find(|u| u.kind == SemanticKind::Class);
        assert!(class_unit.is_some());
        assert_eq!(class_unit.unwrap().name, Some("User".to_string()));

        let methods: Vec<_> = units.iter().filter(|u| u.kind == SemanticKind::Method).collect();
        assert!(!methods.is_empty());
    }

    #[test]
    fn test_extract_interface() {
        let source = r#"
interface User {
    name: string;
    age: number;
}
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Interface);
        assert_eq!(units[0].name, Some("User".to_string()));
    }

    #[test]
    fn test_extract_type_alias() {
        let source = r#"
type UserId = string | number;
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::TypeAlias);
        assert_eq!(units[0].name, Some("UserId".to_string()));
    }

    #[test]
    fn test_extract_arrow_function() {
        let source = r#"
const greet = (name: string): string => {
    return `Hello, ${name}!`;
};
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert_eq!(units[0].name, Some("greet".to_string()));
    }

    #[test]
    fn test_extract_exported_function() {
        let source = r#"
export function publicFunc(): void {
    console.log("public");
}
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
        assert!(units[0].content.contains("export"));
    }

    #[test]
    fn test_extract_enum() {
        let source = r#"
enum Status {
    Active,
    Inactive,
    Pending
}
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Enum);
        assert_eq!(units[0].name, Some("Status".to_string()));
    }

    #[test]
    fn test_javascript_function() {
        let source = r#"
function hello() {
    console.log("Hello");
}
"#;
        let tree = parse_javascript(source);
        let extractor = TypeScriptExtractor::new(true);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Function);
    }

    #[test]
    fn test_extract_test_function() {
        let source = r#"
function testUserCreation() {
    expect(createUser()).toBeDefined();
}
"#;
        let tree = parse_typescript(source);
        let extractor = TypeScriptExtractor::new(false);
        let units = extractor.extract(&tree, source.as_bytes());

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].kind, SemanticKind::Test);
    }
}
