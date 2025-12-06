# Phase 1: C/C++ Language Support Research via Tree-sitter

**Date:** 2025-12-06
**Status:** Research Complete
**Objective:** Analyze architecture for adding C/C++ language support to CodeRAG's AST-based semantic extraction system

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current Architecture Overview](#current-architecture-overview)
3. [C/C++ AST Node Types Reference](#cc-ast-node-types-reference)
4. [File Extension Mapping](#file-extension-mapping)
5. [Example Extractor Code Structure](#example-extractor-code-structure)
6. [Template for Creating New Extractors](#template-for-creating-new-extractors)
7. [Implementation Roadmap](#implementation-roadmap)

---

## Executive Summary

CodeRAG currently supports 7 languages via Tree-sitter: Rust, Python, JavaScript, TypeScript, Go, and Java. The AST chunking system is designed to be extensible through the `SemanticExtractor` trait and `ExtractorRegistry`.

### Key Findings:

- **Architecture is ready for C/C++ support** with minimal changes required
- **Tree-sitter provides high-quality C/C++ grammars** (250+ and 109+ code snippets respectively)
- **C and C++ should be separate extractors** due to significant semantic differences
- **Unified registration approach** works well for language families (TypeScript/JavaScript pattern)
- **Primary challenge:** Handling C++'s complex features (templates, namespaces, inheritance)

---

## Current Architecture Overview

### System Components

#### 1. ParserPool (`src/indexer/ast_chunker/parser_pool.rs`)

Manages tree-sitter parsers for multiple languages:

```rust
pub fn new() -> Self {
    let mut pool = Self {
        parsers: HashMap::new(),
        languages: HashMap::new(),
    };

    // Current language registrations
    pool.register_language("rust", tree_sitter_rust::LANGUAGE.into());
    pool.register_language("python", tree_sitter_python::LANGUAGE.into());
    pool.register_language("javascript", tree_sitter_javascript::LANGUAGE.into());
    pool.register_language("typescript", tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into());
    pool.register_language("tsx", tree_sitter_typescript::LANGUAGE_TSX.into());
    pool.register_language("go", tree_sitter_go::LANGUAGE.into());
    pool.register_language("java", tree_sitter_java::LANGUAGE.into());

    pool
}
```

**To add C/C++:**
- Add `tree-sitter-c` and `tree-sitter-cpp` as Cargo dependencies
- Register both languages in `ParserPool::new()`
- Update `detect_language_from_extension()` with new extensions

#### 2. Language Detection (`src/indexer/ast_chunker/parser_pool.rs`)

Current extension detection:
```rust
pub fn detect_language_from_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "go" => Some("go"),
        "java" => Some("java"),
        _ => None,
    }
}
```

#### 3. SemanticExtractor Trait (`src/indexer/ast_chunker/extractors/mod.rs`)

Interface all language extractors implement:

```rust
pub trait SemanticExtractor: Send + Sync {
    fn language_id(&self) -> &'static str;
    fn extract(&self, tree: &Tree, source: &[u8]) -> Vec<SemanticUnit>;
    fn target_node_types(&self) -> &[&'static str];
}
```

#### 4. SemanticKind Enum

Supported semantic unit types:

```rust
pub enum SemanticKind {
    Function,
    Method,
    Struct,
    Class,
    Trait,
    Interface,
    Enum,
    Impl,
    Module,
    Constant,
    TypeAlias,
    Macro,
    Test,
    Block,
}
```

#### 5. SemanticUnit Structure

Output structure for extracted code units:

```rust
pub struct SemanticUnit {
    pub kind: SemanticKind,
    pub name: Option<String>,
    pub content: String,
    pub docs: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub signature: Option<String>,
    pub parent: Option<String>,
}
```

#### 6. ExtractorRegistry Pattern (`src/indexer/ast_chunker/extractors/mod.rs`)

Central registry for all extractors:

```rust
impl ExtractorRegistry {
    pub fn new() -> Self {
        let mut registry = Self { extractors: HashMap::new() };

        registry.register(Box::new(RustExtractor));
        registry.register(Box::new(PythonExtractor));
        registry.register(Box::new(TypeScriptExtractor::new(false)));
        registry.register(Box::new(GoExtractor));
        registry.register(Box::new(JavaExtractor));

        // Variant registration pattern
        registry.extractors.insert(
            "javascript".to_string(),
            Box::new(TypeScriptExtractor::new(true)),
        );

        registry
    }
}
```

---

## C/C++ AST Node Types Reference

### C Language Node Types

Based on Tree-sitter C grammar analysis:

#### Declarations & Definitions
- `function_definition` - Function with body and implementation
- `declaration` - Variable/function declarations
- `type_definition` - `typedef` statements
- `struct_specifier` - Struct declarations
- `union_specifier` - Union declarations
- `enum_specifier` - Enum declarations
- `field_declaration_list` - Struct/union members
- `field_identifier` - Individual field/member name

#### Function-Related
- `function_declarator` - Function signature
- `parameter_list` - Function parameters
- `parameter_declaration` - Individual parameter
- `variadic_parameter` - `...` varargs

#### Type Specifiers
- `primitive_type` - Built-in types (int, char, float, etc.)
- `type_identifier` - User-defined type names
- `pointer_declarator` - Pointer type annotation
- `array_declarator` - Array notation
- `storage_class_specifier` - `static`, `extern`, `auto`, `register`
- `type_qualifier` - `const`, `volatile`, `restrict`

#### Statements
- `compound_statement` - Block `{ ... }`
- `if_statement` - Conditional statement
- `while_statement` - While loop
- `for_statement` - For loop
- `switch_statement` - Switch/case
- `case_statement` - Case label
- `labeled_statement` - Labeled goto target
- `return_statement` - Return statement
- `break_statement` - Break statement
- `continue_statement` - Continue statement

#### Special Features
- `linkage_specification` - `extern "C"` blocks
- `attribute_declaration` - `__attribute__` GCC/Clang extensions
- `gnu_asm_expression` - Inline assembly

### C++ Language Node Types (Additional to C)

C++ extends C with additional node types:

#### Class/OOP Features
- `class_specifier` - Class declarations
- `class_declaration` - Forward declarations
- `field_declaration_list` - Class members
- `virtual_specifier` - `virtual`, `override`, `final` keywords
- `access_specifier` - `public`, `private`, `protected`
- `destructor_name` - `~ClassName` syntax

#### Templates
- `template_declaration` - Template declarations
- `template_parameter_list` - Template parameters
- `type_parameter_declaration` - Template type parameters
- `template_argument_list` - Template instantiation arguments
- `template_type` - Generic template usage
- `template_template_parameter_declaration` - Template template parameters
- `variadic_type_parameter_declaration` - Variadic templates
- `variadic_parameter_declaration` - Variadic function parameters

#### Namespaces & Using
- `namespace_definition` - `namespace` blocks
- `namespace_identifier` - Qualified namespace names
- `using_declaration` - `using` directives
- `qualified_identifier` - Scope-qualified names (e.g., `std::vector`)

#### Modern C++ Features
- `placeholder_type_specifier` - `auto` and `decltype` types
- `alias_declaration` - `using` type aliases
- `concept_definition` - Concepts (C++20)
- `requires_clause` - Constraints (C++20)
- `attribute_declaration` - C++ attributes `[[...]]`

#### Advanced Features
- `operator_overload` - Operator `operator+` syntax
- `conversion_operator` - `operator Type()` syntax
- `default_member_initializer` - In-class initialization
- `noexcept_specifier` - Exception specifiers
- `explicit_function_declaration` - `explicit` keyword
- `consteval`, `constexpr`, `constinit` - Compile-time specifiers

---

## File Extension Mapping

### Current Mapping
Located in `src/indexer/ast_chunker/parser_pool.rs`:

```rust
pub fn detect_language_from_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "go" => Some("go"),
        "java" => Some("java"),
        _ => None,
    }
}
```

### Proposed C/C++ Extension Mapping

```rust
// C language extensions
"c" => Some("c"),

// C++ language extensions
"cc" | "cxx" | "cpp" | "c++" => Some("cpp"),

// Header files - require disambiguation
"h" => Some("c"),        // Assume C unless context suggests C++
"hpp" | "hxx" | "h++" => Some("cpp"),
"hh" => Some("cpp"),
```

**Challenge:** `.h` files are ambiguous (can be C or C++). Options:
1. Always assume `.h` → C (Conservative, may miss C++ files)
2. Always assume `.h` → C++ (Risky, may parse C incorrectly)
3. Use content heuristics (check for C++ keywords like `class`, `template`)
4. Make configurable per project

**Recommendation:** Default to C for `.h`, with opt-in configuration for C++.

---

## Example Extractor Code Structure

### Reference: Rust Extractor Pattern
From `src/indexer/ast_chunker/extractors/rust.rs`

The Rust extractor demonstrates the standard pattern:

1. **Struct Definition**
```rust
pub struct RustExtractor;

impl SemanticExtractor for RustExtractor {
    fn language_id(&self) -> &'static str { "rust" }
    fn target_node_types(&self) -> &[&'static str] { ... }
    fn extract(&self, tree: &Tree, source: &[u8]) -> Vec<SemanticUnit> { ... }
}
```

2. **Recursive AST Traversal**
```rust
fn extract_from_node(
    &self,
    cursor: &mut TreeCursor,
    source: &[u8],
    units: &mut Vec<SemanticUnit>,
    parent_context: Option<&str>,
) {
    let node = cursor.node();

    // Try to extract this node
    if let Some(unit) = self.extract_unit(&node, source, parent_context) {
        units.push(unit);
    }

    // Handle context (e.g., impl blocks for Rust)
    let new_context = if node.kind() == "impl_item" {
        self.get_impl_target(&node, source)
    } else {
        parent_context.map(|s| s.to_string())
    };

    // Recurse into children
    if cursor.goto_first_child() {
        loop {
            self.extract_from_node(cursor, source, units, new_context.as_deref());
            if !cursor.goto_next_sibling() { break; }
        }
        cursor.goto_parent();
    }
}
```

3. **Unit Extraction**
```rust
fn extract_unit(
    &self,
    node: &Node,
    source: &[u8],
    parent_context: Option<&str>,
) -> Option<SemanticUnit> {
    let kind = match node.kind() {
        "function_item" => SemanticKind::Function,
        "struct_item" => SemanticKind::Struct,
        "enum_item" => SemanticKind::Enum,
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
```

4. **Helper Methods**
```rust
fn get_name(&self, node: &Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| node_text(&n, source).to_string())
}

fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
    // Build function signature from components
    if node.kind() != "function_item" { return None; }
    // ... implementation details
}

fn get_docs(&self, node: &Node, source: &[u8]) -> Option<String> {
    // Collect documentation comments (/// or //!)
    // ... implementation details
}
```

---

## Template for Creating New Extractors

### File: `src/indexer/ast_chunker/extractors/c.rs`

```rust
//! C-specific semantic extractor.
//!
//! Extracts: function_definition, struct_specifier, union_specifier, enum_specifier,
//! type_definition, field_declaration_list

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

        self.extract_from_node(&mut cursor, source, &mut units, None);

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
        parent_context: Option<&str>,
    ) {
        let node = cursor.node();

        // Try to extract a semantic unit from this node
        if let Some(unit) = self.extract_unit(&node, source, parent_context) {
            units.push(unit);
        }

        // C doesn't have strong parent contexts like impl blocks,
        // but we might track file-level vs. nested definitions
        // For now, don't propagate context

        // Recurse into children
        if cursor.goto_first_child() {
            loop {
                self.extract_from_node(cursor, source, units, None);
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
        _parent_context: Option<&str>,
    ) -> Option<SemanticUnit> {
        let kind = match node.kind() {
            "function_definition" => SemanticKind::Function,
            "struct_specifier" => SemanticKind::Struct,
            "union_specifier" => {
                // Treat unions similarly to structs
                SemanticKind::Struct
            }
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
                // In function_definition, the name is in function_declarator
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        if cursor.node().kind() == "function_declarator" {
                            let decl = cursor.node();
                            // Function declarator's first child is the identifier
                            if let Some(name_node) = decl.child(0) {
                                return Some(node_text(&name_node, source).to_string());
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
                // Struct/union/enum may have a name field
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "type_definition" => {
                // Type definitions have a declarator for the new type name
                node.child_by_field_name("declarator")
                    .map(|n| node_text(&n, source).to_string())
            }
            _ => None,
        }
    }

    /// Get function signature.
    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        if node.kind() != "function_definition" {
            return None;
        }

        // Build signature from return type, name, and parameters
        let mut parts = Vec::new();
        let mut found_params = false;

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "primitive_type" | "type_identifier" => {
                        parts.push(node_text(&child, source).to_string());
                    }
                    "function_declarator" => {
                        // Extract function name and parameters
                        let func_text = node_text(&child, source);
                        parts.push(func_text.to_string());
                        found_params = true;
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if found_params && !parts.is_empty() {
            Some(parts.join(" ").trim().to_string())
        } else {
            None
        }
    }

    /// Get documentation/comments associated with a node.
    fn get_docs(&self, node: &Node, source: &[u8]) -> Option<String> {
        // Look for preceding comments (// or /* */)
        let mut docs = Vec::new();

        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "comment" {
                docs.push(node_text(&prev, source).to_string());
            }
        }

        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
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

        assert!(units.iter().any(|u| u.kind == SemanticKind::Struct));
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

        assert!(units.iter().any(|u| u.kind == SemanticKind::Enum));
    }
}
```

### File: `src/indexer/ast_chunker/extractors/cpp.rs`

```rust
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
            "field_declaration",
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
            self.get_namespace_name(node, source)
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
        _parent_context: Option<&str>,
    ) -> Option<SemanticUnit> {
        let kind = match node.kind() {
            "function_definition" => SemanticKind::Function,
            "class_specifier" => SemanticKind::Class,
            "struct_specifier" => SemanticKind::Struct,
            "enum_specifier" => SemanticKind::Enum,
            "template_declaration" => {
                // Template could wrap function, class, or other
                // For now, treat as its own kind or inspect children
                SemanticKind::Block
            }
            "namespace_definition" => SemanticKind::Module,
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

    fn get_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_definition" => {
                // Similar to C: find identifier in function_declarator
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        if cursor.node().kind() == "function_declarator" {
                            if let Some(name_node) = cursor.node().child(0) {
                                return Some(node_text(&name_node, source).to_string());
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
            "enum_specifier" => {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            "namespace_definition" => {
                node.child_by_field_name("name")
                    .map(|n| node_text(&n, source).to_string())
            }
            _ => None,
        }
    }

    fn get_namespace_name(&self, node: &Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .map(|n| node_text(&n, source).to_string())
    }

    fn get_signature(&self, node: &Node, source: &[u8]) -> Option<String> {
        if node.kind() == "function_definition" {
            // Similar to C: extract return type, name, and parameters
            let mut parts = Vec::new();
            let mut found_params = false;

            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    match child.kind() {
                        "primitive_type" | "type_identifier" | "qualified_identifier" => {
                            parts.push(node_text(&child, source).to_string());
                        }
                        "function_declarator" => {
                            parts.push(node_text(&child, source).to_string());
                            found_params = true;
                        }
                        "template_parameter_list" => {
                            parts.push(node_text(&child, source).to_string());
                        }
                        _ => {}
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }

            if found_params && !parts.is_empty() {
                Some(parts.join(" ").trim().to_string())
            } else {
                None
            }
        } else if node.kind() == "template_declaration" {
            // For templates, include template parameters in signature
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                if cursor.node().kind() == "template_parameter_list" {
                    return Some(node_text(&cursor.node(), source).to_string());
                }
            }
            None
        } else {
            None
        }
    }

    fn get_docs(&self, node: &Node, source: &[u8]) -> Option<String> {
        let mut docs = Vec::new();

        if let Some(prev) = node.prev_sibling() {
            if prev.kind() == "comment" {
                docs.push(node_text(&prev, source).to_string());
            }
        }

        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
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

        assert!(units.iter().any(|u| u.kind == SemanticKind::Class));
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

        // Should find template or class or both
        assert!(!units.is_empty());
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

        assert!(units.iter().any(|u| u.kind == SemanticKind::Module));
    }
}
```

---

## Template for Creating New Extractors

### Registration Steps

#### Step 1: Update `Cargo.toml`

Add tree-sitter grammar crates:

```toml
[dependencies]
tree-sitter = "0.20"
tree-sitter-c = "0.20"
tree-sitter-cpp = "0.20"
# ... other dependencies
```

#### Step 2: Update `parser_pool.rs`

Add languages to `ParserPool::new()`:

```rust
impl ParserPool {
    pub fn new() -> Self {
        let mut pool = Self {
            parsers: HashMap::new(),
            languages: HashMap::new(),
        };

        // ... existing registrations ...

        // Add C and C++
        pool.register_language("c", tree_sitter_c::LANGUAGE.into());
        pool.register_language("cpp", tree_sitter_cpp::LANGUAGE.into());

        pool
    }
}
```

Update extension detection:

```rust
pub fn detect_language_from_extension(ext: &str) -> Option<&'static str> {
    match ext {
        // ... existing mappings ...
        "c" => Some("c"),
        "cc" | "cxx" | "cpp" | "c++" => Some("cpp"),
        "h" => Some("c"),        // Ambiguous, default to C
        "hpp" | "hxx" | "h++" => Some("cpp"),
        _ => None,
    }
}
```

#### Step 3: Create Extractor Files

Create:
- `src/indexer/ast_chunker/extractors/c.rs`
- `src/indexer/ast_chunker/extractors/cpp.rs`

Add module declarations in `src/indexer/ast_chunker/extractors/mod.rs`:

```rust
pub mod c;
pub mod cpp;

pub use c::CExtractor;
pub use cpp::CppExtractor;
```

#### Step 4: Register Extractors

Update `ExtractorRegistry::new()`:

```rust
impl ExtractorRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            extractors: HashMap::new(),
        };

        // ... existing registrations ...

        registry.register(Box::new(CExtractor));
        registry.register(Box::new(CppExtractor));

        registry
    }
}
```

#### Step 5: Add Tests

Create comprehensive tests in each extractor file (see test sections above).

---

## Implementation Roadmap

### Phase 1: Foundation (Weeks 1-2)
- [x] Research AST architecture and requirements
- [ ] Add tree-sitter-c and tree-sitter-cpp dependencies
- [ ] Implement basic CExtractor for C functions, structs, enums
- [ ] Implement basic CppExtractor for C++ classes, functions, templates
- [ ] Write unit tests for basic extraction

### Phase 2: Enhancement (Weeks 3-4)
- [ ] Add documentation comment extraction for both languages
- [ ] Improve signature extraction (parameters, return types)
- [ ] Handle C++ namespaces and qualified identifiers
- [ ] Support C++ template specialization metadata
- [ ] Add integration tests with real C/C++ projects

### Phase 3: Optimization (Weeks 5-6)
- [ ] Performance profiling on large C/C++ files
- [ ] Handle edge cases (macros, preprocessor directives)
- [ ] Improve context tracking for nested structures
- [ ] Add configuration for .h file disambiguation
- [ ] Documentation and example updates

### Phase 4: Polish (Weeks 7-8)
- [ ] Create comprehensive test suite
- [ ] Update CLI to show C/C++ support
- [ ] Benchmark vs. other extractors
- [ ] Address any remaining edge cases
- [ ] Prepare for production release

---

## Key Considerations

### C vs. C++ Distinction

**Why separate extractors:**
1. Different target node types and grammar
2. C++ has templates, namespaces, classes - C doesn't
3. Method extraction differs significantly
4. Header file handling differs

**Why similar structure:**
1. Both compile to similar intermediate representations
2. Large overlap in function/struct extraction
3. Code reuse for common patterns

### .h File Handling

**Recommendation:**
- Default `.h` → C language
- Add configuration option: `--cpp-headers` or `cpp_header_extensions: ["h"]` in config
- Consider heuristic: if file contains `class`, `template`, `namespace` → treat as C++

### Template Complexity

C++ templates are sophisticated. For Phase 1, recommend:
1. Extract template declarations as-is (full text)
2. Mark as `SemanticKind::Block` or new `SemanticKind::Template`
3. Store template parameters in signature field
4. Future enhancement: parse template specializations

### Macro Handling

Tree-sitter parses preprocessor directives but doesn't expand macros:
1. Will see `#define` and `#ifdef` nodes
2. Can extract macro definitions as constants
3. Cannot resolve macro expansions within code
4. This is acceptable limitation; document in release notes

---

## References

### Tree-sitter Documentation
- **C Grammar:** /tree-sitter/tree-sitter-c (109 code snippets)
- **C++ Grammar:** /tree-sitter/tree-sitter-cpp (250 code snippets)
- **Core Tree-sitter:** High-quality reference implementations

### Current CodeRAG Implementation
- **ParserPool:** `src/indexer/ast_chunker/parser_pool.rs` (47-95 lines)
- **Extractors:** `src/indexer/ast_chunker/extractors/` (7 language implementations)
- **Trait Definition:** `src/indexer/ast_chunker/extractors/mod.rs` (118-175 lines)
- **Rust Example:** `src/indexer/ast_chunker/extractors/rust.rs` (Complete reference implementation)

### Node Kind Reference
**C specifics:** Comprehensive S-expression examples in tree-sitter-c test corpus
**C++ specifics:** Template, class, and namespace examples in tree-sitter-cpp test corpus

---

## Next Steps

1. **Validate AST node types** against actual .c/.cpp files
2. **Create prototype extractors** based on template
3. **Build test suite** with real-world C/C++ code samples
4. **Performance test** on large codebases (Linux kernel, LLVM, etc.)
5. **Document limitations** (macros, preprocessor directives)

