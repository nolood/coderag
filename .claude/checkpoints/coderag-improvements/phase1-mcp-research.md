# Phase 1: MCP Tool Implementation Research

**Date**: 2025-12-06
**Research Focus**: MCP tool implementation patterns for symbol search capabilities

---

## 1. How to Add New MCP Tools

### 1.1 Tool Definition Pattern

CodeRAG uses the RMCP Rust SDK with macro-based tool definition. The pattern is:

```rust
#[tool_router]
impl CodeRagServer {
    #[tool(
        name = "tool_name",
        description = "Human-readable tool description"
    )]
    async fn tool_function(
        &self,
        Parameters(req): Parameters<RequestType>,
    ) -> Result<CallToolResult, McpError> {
        // Implementation
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }
}
```

**Key Components**:
- `#[tool_router]` macro on impl block enables tool routing
- `#[tool(...)]` attribute defines tool metadata
- `Parameters<T>` wrapper for request deserialization
- `RequestType` must implement `Deserialize + JsonSchema`

### 1.2 Request Parameter Patterns

Current pattern in `/home/nolood/general/coderag/src/mcp/server.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "Natural language query to search for relevant code")]
    query: String,

    #[schemars(description = "Maximum number of results to return (default: 10)")]
    limit: Option<usize>,
}
```

**Best Practices**:
- Use `#[schemars(description = "...")]` for parameter documentation
- Support optional parameters with `Option<T>`
- Document default values in schema
- Implement both `Debug`, `Deserialize`, and `JsonSchema`

### 1.3 Tool Handler Trait Stack

From RMCP documentation:
- `CallToolHandler<S, A>` - Core trait for tool execution
- `CallToolHandlerExt<S, A>` - Extension trait for fluent configuration
- `WithToolAttr<C, S, A>` - Wrapper for adding attributes
- `IntoToolRoute<S, A>` - Conversion to tool routes

### 1.4 ServerHandler Implementation

Required trait in `/home/nolood/general/coderag/src/mcp/server.rs`:

```rust
#[tool_handler]
impl ServerHandler for CodeRagServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "coderag".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                // ...
            },
            instructions: Some("Tool descriptions...".into()),
        }
    }
}
```

---

## 2. Existing AST Extraction Capabilities

### 2.1 SemanticUnit Structure

From `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/mod.rs`:

```rust
pub struct SemanticUnit {
    pub kind: SemanticKind,           // Function, Class, Struct, etc.
    pub name: Option<String>,         // Function/class name
    pub content: String,              // Full source code
    pub docs: Option<String>,         // Documentation strings
    pub start_line: usize,            // 1-indexed
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub signature: Option<String>,    // Function signature
    pub parent: Option<String>,       // Parent context (class, impl block)
}
```

### 2.2 SemanticKind Enum

14 semantic unit types are supported:

```
Function, Method, Struct, Class, Trait, Interface, Enum, Impl,
Module, Constant, TypeAlias, Macro, Test, Block
```

**Language-Specific Extraction**:
- **Rust**: function_item, impl_item, struct_item, enum_item, trait_item, mod_item, const_item, static_item, type_item, macro_definition
- **Python**: function_definition, class_definition, decorated_definition
- **Java**: class_declaration, interface_declaration, enum_declaration, method_declaration, constructor_declaration, field_declaration
- **Go**: function_declaration, method_declaration, type_declaration, const_declaration, var_declaration
- **TypeScript/JavaScript**: function_declaration, class_declaration, method_definition, interface_declaration, type_alias_declaration, arrow_function, export_statement

### 2.3 Extractor Registry

From `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/mod.rs`:

```rust
pub trait SemanticExtractor: Send + Sync {
    fn language_id(&self) -> &'static str;
    fn extract(&self, tree: &Tree, source: &[u8]) -> Vec<SemanticUnit>;
    fn target_node_types(&self) -> &[&'static str];
}

pub struct ExtractorRegistry {
    extractors: HashMap<String, Box<dyn SemanticExtractor>>,
}
```

**Supported Languages**: Rust, Python, TypeScript, JavaScript, Go, Java

### 2.4 AST Parsing Infrastructure

From `/home/nolood/general/coderag/src/indexer/ast_chunker/mod.rs`:

- **ParserPool**: Manages tree-sitter parsers for concurrent parsing
- **Language Detection**: File extension → language mapping (rs, py, js, jsx, ts, tsx, go, java)
- **Fallback Mechanism**: Line-based chunking when AST parsing fails
- **Token Estimation**: Approximate 4 chars per token for size constraints

---

## 3. Symbol Index Structure Recommendations

### 3.1 Current Chunk Structure

From chunks stored in storage:

```rust
pub struct Chunk {
    pub content: String,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
    pub semantic_kind: Option<SemanticKind>,  // NEW: Semantic type
    pub name: Option<String>,                  // NEW: Symbol name
    pub signature: Option<String>,             // NEW: Function/method signature
    pub parent: Option<String>,                // NEW: Parent context
}
```

### 3.2 Proposed Symbol Index Schema

For a dedicated symbol search capability:

```rust
pub struct SymbolIndex {
    // Unique identifier: file_path + start_line + symbol_name
    pub symbol_id: String,

    // Basic metadata
    pub name: String,
    pub kind: SemanticKind,
    pub language: String,

    // Location information
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub column: Option<usize>,        // For precise IDE navigation

    // Semantic information
    pub signature: Option<String>,    // Function signature, type definition
    pub parent: Option<String>,       // Scope: class name, module path, impl block
    pub docs: Option<String>,         // Documentation/comments
    pub visibility: Option<String>,   // public, private, protected

    // Content and embeddings
    pub declaration: String,          // Just the signature/declaration
    pub full_content: String,         // Complete implementation
    pub embedding: Vec<f32>,          // Semantic embedding for search

    // Relationships
    pub references: Vec<String>,      // Other symbols this references
    pub referenced_by: Vec<String>,   // Symbols that reference this
    pub type_info: Option<String>,    // Return type, parameter types
}
```

### 3.3 Indexing Strategy

1. **Extract** all semantic units from each file using AST extractors
2. **Enrich** with type information and relationships (requires additional analysis)
3. **Embed** both signature and full content for multi-modal search
4. **Store** in structured format with reverse indexing for relationships
5. **Cache** embeddings for fast retrieval

---

## 4. Tool Parameter Patterns

### 4.1 Pattern 1: Simple Query + Options

**Usage**: General search, list operations

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchRequest {
    #[schemars(description = "...")]
    query: String,

    #[schemars(description = "...")]
    limit: Option<usize>,

    #[schemars(description = "...")]
    offset: Option<usize>,
}
```

### 4.2 Pattern 2: Structured Filters

**Usage**: Advanced symbol search with criteria

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolSearchRequest {
    #[schemars(description = "Symbol name or pattern to search for")]
    query: String,

    #[schemars(description = "Filter by semantic kind")]
    kind: Option<String>,  // "function", "class", "struct", etc.

    #[schemars(description = "Filter by programming language")]
    language: Option<String>,

    #[schemars(description = "Filter by file path pattern")]
    file_pattern: Option<String>,

    #[schemars(description = "Maximum number of results")]
    limit: Option<usize>,

    #[schemars(description = "Search mode: 'exact', 'fuzzy', 'semantic'")]
    mode: Option<String>,
}
```

### 4.3 Pattern 3: Batch Operations

**Usage**: Multiple operations in one request

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BatchSymbolRequest {
    #[schemars(description = "List of symbol queries")]
    queries: Vec<SymbolQuery>,

    #[schemars(description = "Whether to stop on first error")]
    fail_fast: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SymbolQuery {
    pub symbol_name: String,
    pub file_path: Option<String>,
}
```

### 4.4 Response Pattern

**Consistent structure** for all tools:

```rust
pub struct SymbolSearchResult {
    pub symbol_name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub documentation: Option<String>,
    pub relevance_score: f32,  // 0.0 to 1.0
}

pub struct ToolResponse {
    pub success: bool,
    pub results: Vec<SymbolSearchResult>,
    pub total: usize,
    pub execution_time_ms: u64,
    pub error: Option<String>,
}
```

---

## 5. RMCP Library Capabilities Summary

### 5.1 Tool Router Architecture

From RMCP source analysis:

**ToolRouter<S>**:
- Manages registered tools in HashMap: `map: HashMap<Cow<'static, str>, ToolRoute<S>>`
- Supports dynamic tool registration
- Provides introspection: `list_all()` returns all tool metadata
- Async tool execution with error handling

**ToolRoute<S>**:
- Encapsulates tool metadata and call handler
- `attr: Tool` contains name, description, input_schema, annotations
- `call: Arc<DynCallToolHandler<S>>` is the actual execution function
- Cloneable for concurrent access

### 5.2 Tool Definition Macros

**Key Macros** from RMCP:

- `#[tool_router]` on impl block - Generates tool routing infrastructure
- `#[tool(...)]` on methods - Defines individual tool metadata
- `#[tool_handler]` on ServerHandler impl - Marks tool handler trait implementation
- `Parameters<T>` wrapper - Auto-deserializes JSON request parameters

### 5.3 ServerHandler Interface

**Required Method**:
```rust
fn get_info(&self) -> ServerInfo {
    // Protocol version, capabilities, metadata, instructions
}
```

**Server Info Components**:
- `protocol_version`: Version of MCP protocol (currently V_2024_11_05)
- `capabilities`: `ServerCapabilities::builder().enable_tools().build()`
- `server_info`: Name, version, title, icons, website
- `instructions`: Human-readable guide for available tools

---

## 6. Implementation Roadmap for Symbol Search Tool

### 6.1 New Tool: `search_symbol`

```rust
#[tool(
    name = "search_symbol",
    description = "Search for specific symbols (functions, classes, etc.) by name with filtering options"
)]
async fn search_symbol(
    &self,
    Parameters(req): Parameters<SymbolSearchRequest>,
) -> Result<CallToolResult, McpError>
```

**Features**:
- Query by symbol name
- Filter by kind (function, class, struct, etc.)
- Filter by language
- Support for exact, fuzzy, and semantic matching

### 6.2 New Tool: `get_symbol_definition`

```rust
#[tool(
    name = "get_symbol_definition",
    description = "Get the full definition and documentation of a specific symbol"
)]
async fn get_symbol_definition(
    &self,
    Parameters(req): Parameters<SymbolDefinitionRequest>,
) -> Result<CallToolResult, McpError>
```

### 6.3 New Tool: `list_symbols_in_file`

```rust
#[tool(
    name = "list_symbols_in_file",
    description = "List all symbols (functions, classes, etc.) defined in a specific file"
)]
async fn list_symbols_in_file(
    &self,
    Parameters(req): Parameters<ListSymbolsRequest>,
) -> Result<CallToolResult, McpError>
```

---

## 7. Key Integration Points

### 7.1 Storage Layer Integration

Current: `/home/nolood/general/coderag/src/storage/`
- Extend to store symbol metadata alongside chunks
- Index by symbol_id for fast lookup
- Support relationship queries

### 7.2 Search Engine Integration

Current: `/home/nolood/general/coderag/src/search/`
- Leverage existing SearchEngine for embeddings
- Add symbol-specific search methods
- Support semantic + keyword hybrid search

### 7.3 AST Extraction Integration

Current: `/home/nolood/general/coderag/src/indexer/ast_chunker/`
- Already extracts semantic units with symbols
- Use existing SemanticExtractor trait
- Enhance with type information extraction

---

## 8. Challenges and Solutions

### 8.1 Challenge: Symbol Relationship Tracking

**Problem**: Maintaining accurate cross-symbol references (who calls whom)

**Solution Options**:
1. Post-processing pass after indexing to analyze call patterns
2. Incremental relationship tracking during embedding generation
3. Use tree-sitter queries for language-specific relationship patterns

### 8.2 Challenge: Type Information Extraction

**Problem**: Extracting accurate type signatures across languages

**Solution**:
- Leverage tree-sitter for type nodes
- Language-specific extractors (already in place)
- Store type annotations from AST

### 8.3 Challenge: Symbol Scope and Visibility

**Problem**: Distinguishing public/private and scope boundaries

**Solution**:
- Parse visibility modifiers from AST
- Track scope hierarchy in parent field
- Store visibility in symbol metadata

---

## 9. References and Code Locations

### Core Implementation Files

1. **MCP Server**: `/home/nolood/general/coderag/src/mcp/server.rs`
   - Current tools: search, list_files, get_file
   - Tool parameter patterns
   - ServerHandler implementation

2. **HTTP Transport**: `/home/nolood/general/coderag/src/mcp/http.rs`
   - HTTP/SSE transport configuration
   - Service factory pattern

3. **AST Chunking**: `/home/nolood/general/coderag/src/indexer/ast_chunker/mod.rs`
   - AstChunker implementation
   - Language detection
   - Fallback mechanisms

4. **Extractors**: `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/`
   - Language-specific extractors (Rust, Python, Java, Go, TypeScript)
   - SemanticUnit and SemanticKind definitions
   - ExtractorRegistry

5. **Storage**: `/home/nolood/general/coderag/src/storage/`
   - Chunk storage
   - File listing
   - Query interface

### RMCP Dependencies

- **Library**: `/websites/rs-rmcp` (High reputation, 1615 code snippets)
- **Official**: `/websites/rs_rmcp_rmcp` (Official SDK, 835 code snippets)
- **Key Concepts**:
  - ToolRouter: Tool registration and routing
  - ToolRoute: Individual tool definition
  - CallToolHandler: Trait for tool execution
  - ServerHandler: Server-level capabilities

---

## 10. Next Steps for Implementation

1. **Design** symbol index schema and storage structure
2. **Implement** symbol metadata extraction from existing AST capabilities
3. **Create** new MCP tools: search_symbol, get_symbol_definition, list_symbols_in_file
4. **Test** with various languages (Rust, Python, TypeScript, Go, Java)
5. **Optimize** symbol search with embeddings and hybrid search
6. **Document** symbol search API and patterns

---

**Document Generated**: 2025-12-06
**Research Focus Area**: MCP server tool patterns, AST symbol extraction, symbol indexing strategies
