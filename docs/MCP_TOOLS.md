# MCP Tools Documentation

CodeRAG provides a comprehensive set of MCP (Model Context Protocol) tools for LLMs to interact with your codebase. These tools enable semantic search, symbol navigation, and code exploration.

## Available MCP Tools

### 1. search
Semantic code search using vector embeddings and BM25.

**Request:**
```json
{
  "query": "authentication middleware",
  "limit": 10
}
```

**Response:**
```json
{
  "results": [
    {
      "file_path": "src/middleware/auth.rs",
      "start_line": 45,
      "end_line": 89,
      "content": "pub fn authenticate(req: Request) -> Result<User> {...}",
      "score": 0.92,
      "file_header": "// First 50 lines of the file for context..."
    }
  ]
}
```

**Features:**
- Hybrid search combining vector and keyword matching
- File header injection (first 50 lines) for context
- Relevance scoring
- Language-aware code chunking

### 2. list_files
List indexed files with optional glob pattern filtering.

**Request:**
```json
{
  "pattern": "**/*.rs"
}
```

**Response:**
```json
{
  "files": [
    {
      "path": "src/main.rs",
      "size": 2048,
      "language": "rust",
      "chunks": 5,
      "last_modified": "2024-01-20T10:30:00Z"
    }
  ]
}
```

**Pattern Examples:**
- `*.rs` - All Rust files in root
- `**/*.ts` - All TypeScript files recursively
- `src/**/*.{js,jsx}` - JavaScript files in src
- `tests/**/*_test.go` - Go test files

### 3. get_file
Retrieve complete file contents.

**Request:**
```json
{
  "path": "src/main.rs"
}
```

**Response:**
```json
{
  "path": "src/main.rs",
  "content": "// Complete file contents...",
  "language": "rust",
  "size": 2048,
  "chunks": 5
}
```

### 4. find_symbol
Search for symbols (functions, classes, variables) by name.

**Request:**
```json
{
  "query": "processPayment",
  "kind": "function",
  "mode": "exact",
  "limit": 10
}
```

**Parameters:**
- **query**: Symbol name to search for
- **kind**: Symbol type filter
  - `function` - Functions and methods
  - `class` - Classes and structs
  - `variable` - Variables and constants
  - `type` - Type definitions and interfaces
  - `module` - Modules and namespaces
  - `all` - All symbol types (default)
- **mode**: Search mode
  - `exact` - Exact name match
  - `prefix` - Starts with query
  - `contains` - Contains query (default)
  - `fuzzy` - Fuzzy matching
- **limit**: Maximum results (default: 10)

**Response:**
```json
{
  "symbols": [
    {
      "name": "processPayment",
      "kind": "function",
      "file_path": "src/payments/processor.ts",
      "line": 156,
      "column": 8,
      "signature": "async function processPayment(order: Order): Promise<PaymentResult>",
      "context": "export async function processPayment(order: Order): Promise<PaymentResult> {\n  // Implementation...\n}"
    }
  ]
}
```

### 5. list_symbols
List all symbols in a specific file.

**Request:**
```json
{
  "file_path": "src/payments.rs",
  "kind_filter": ["function", "class"]
}
```

**Parameters:**
- **file_path**: Path to the file
- **kind_filter**: Optional array of symbol types to include

**Response:**
```json
{
  "file_path": "src/payments.rs",
  "symbols": [
    {
      "name": "PaymentProcessor",
      "kind": "class",
      "line": 12,
      "children": [
        {
          "name": "new",
          "kind": "function",
          "line": 18
        },
        {
          "name": "process",
          "kind": "function",
          "line": 34
        }
      ]
    }
  ]
}
```

### 6. find_references
Find all references to a symbol across the codebase.

**Request:**
```json
{
  "symbol_name": "User",
  "limit": 50
}
```

**Parameters:**
- **symbol_name**: Name of the symbol to find references for
- **limit**: Maximum number of references (default: 50)

**Response:**
```json
{
  "symbol": "User",
  "references": [
    {
      "file_path": "src/auth/login.rs",
      "line": 45,
      "column": 12,
      "context": "let user: User = User::find_by_email(email)?;",
      "reference_kind": "type_annotation"
    },
    {
      "file_path": "src/models/user.rs",
      "line": 8,
      "column": 1,
      "context": "pub struct User {",
      "reference_kind": "definition"
    }
  ]
}
```

**Reference Kinds:**
- `definition` - Where the symbol is defined
- `import` - Import statements
- `type_annotation` - Type annotations
- `instantiation` - Object creation
- `method_call` - Method invocations
- `property_access` - Property access
- `function_call` - Function calls

## Usage Examples

### Example 1: Understanding a Feature
```javascript
// Find authentication-related code
const authCode = await mcp.search({
  query: "authentication and authorization",
  limit: 20
});

// Find the main auth class
const authClass = await mcp.find_symbol({
  query: "Authenticator",
  kind: "class",
  mode: "exact"
});

// Get the full implementation
const impl = await mcp.get_file({
  path: authClass.symbols[0].file_path
});
```

### Example 2: Exploring API Usage
```javascript
// Find all uses of a specific API
const apiRefs = await mcp.find_references({
  symbol_name: "fetchUserData",
  limit: 100
});

// Get context for each reference
for (const ref of apiRefs.references) {
  const file = await mcp.get_file({ path: ref.file_path });
  // Analyze usage patterns...
}
```

### Example 3: Code Navigation
```javascript
// List all test files
const testFiles = await mcp.list_files({
  pattern: "**/*test*.{js,ts}"
});

// Get symbols from a test file
const testSymbols = await mcp.list_symbols({
  file_path: testFiles.files[0].path,
  kind_filter: ["function"]
});

// Find test implementations
const tests = testSymbols.symbols.filter(s =>
  s.name.startsWith("test") || s.name.startsWith("it")
);
```

## Symbol Search Strategies

### Finding Definitions
```json
{
  "query": "MyClass",
  "kind": "class",
  "mode": "exact"
}
```

### Finding Implementations
```json
{
  "query": "process",
  "kind": "function",
  "mode": "prefix"
}
```

### Finding Variables
```json
{
  "query": "config",
  "kind": "variable",
  "mode": "contains"
}
```

### Finding Type Definitions
```json
{
  "query": "Request",
  "kind": "type",
  "mode": "exact"
}
```

## Performance Characteristics

| Tool | Typical Latency | Scalability |
|------|-----------------|-------------|
| search | <50ms | Excellent (vector index) |
| find_symbol | <10ms | Excellent (indexed) |
| list_symbols | <20ms | Good (AST parsing) |
| find_references | <100ms | Good (depends on codebase) |
| list_files | <5ms | Excellent (cached) |
| get_file | <10ms | Excellent (direct read) |

## Best Practices

### 1. Use Appropriate Search Tools
- **Semantic concepts**: Use `search`
- **Specific symbols**: Use `find_symbol`
- **File exploration**: Use `list_files` + `get_file`
- **Usage analysis**: Use `find_references`

### 2. Combine Tools for Context
```javascript
// First search broadly
const results = await search({ query: "payment processing" });

// Then get specific symbols
const symbols = await list_symbols({
  file_path: results.results[0].file_path
});

// Finally get references
const refs = await find_references({
  symbol_name: symbols.symbols[0].name
});
```

### 3. Use Filters Effectively
- Filter by symbol kind to reduce noise
- Use exact mode when you know the name
- Use patterns for file filtering

### 4. Handle Large Results
```javascript
// Paginate through results
let allRefs = [];
let limit = 100;
let offset = 0;

while (true) {
  const refs = await find_references({
    symbol_name: "User",
    limit: limit,
    offset: offset
  });

  allRefs.push(...refs.references);
  if (refs.references.length < limit) break;
  offset += limit;
}
```

## Integration with LLMs

### Claude Desktop Configuration
```json
{
  "mcpServers": {
    "coderag": {
      "command": "coderag",
      "args": ["serve"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

### Prompting Strategies

**For Code Understanding:**
> "Use find_symbol to locate the main authentication class, then use list_symbols to understand its structure, and finally use find_references to see how it's used throughout the codebase."

**For Refactoring:**
> "First use find_references to find all uses of the old API, then use get_file to examine each usage context, and suggest refactoring changes."

**For Documentation:**
> "Use list_files to find all source files, then list_symbols for each file to create a comprehensive API documentation."

## Error Handling

All tools return errors in a consistent format:

```json
{
  "error": {
    "code": "SYMBOL_NOT_FOUND",
    "message": "No symbol found matching 'MyClass'",
    "details": {
      "query": "MyClass",
      "searched_files": 150
    }
  }
}
```

Common error codes:
- `FILE_NOT_FOUND` - File doesn't exist in index
- `SYMBOL_NOT_FOUND` - No matching symbols
- `INVALID_PATTERN` - Invalid glob pattern
- `INDEX_NOT_READY` - Index still building
- `RATE_LIMITED` - Too many requests

## Advanced Features

### Symbol Hierarchies
The `list_symbols` tool returns hierarchical symbol information:

```json
{
  "symbols": [
    {
      "name": "PaymentModule",
      "kind": "module",
      "children": [
        {
          "name": "PaymentProcessor",
          "kind": "class",
          "children": [
            {
              "name": "process",
              "kind": "function"
            }
          ]
        }
      ]
    }
  ]
}
```

### Cross-Reference Analysis
Combine tools for dependency analysis:

```javascript
// Find all classes that use a specific interface
const interface = await find_symbol({
  query: "PaymentGateway",
  kind: "type"
});

const implementations = await search({
  query: "implements PaymentGateway"
});
```

### Code Metrics
Use symbol information for metrics:

```javascript
// Count functions per file
const files = await list_files({ pattern: "**/*.ts" });
let metrics = {};

for (const file of files.files) {
  const symbols = await list_symbols({
    file_path: file.path,
    kind_filter: ["function"]
  });
  metrics[file.path] = symbols.symbols.length;
}
```