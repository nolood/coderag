# Phase 2: Symbol Search MCP Tool Architecture

**Date**: 2025-12-06
**Architecture Focus**: Symbol search capabilities extending CodeRAG's existing AST extraction

---

## 1. Architecture Overview

### High-Level Decision
**Recommendation: Option B - Enhanced Chunks with Symbol Metadata**

The architecture leverages existing AST extraction capabilities while adding dedicated symbol indexing for fast lookups. This hybrid approach minimizes storage overhead while providing rich symbol search capabilities.

### Key Architectural Decisions
1. **Storage Strategy**: Enhance existing chunk storage with symbol metadata rather than creating a separate table
2. **Index Structure**: Build in-memory symbol index on startup for fast lookups
3. **Reference Tracking**: Lazy loading of references to avoid complex cross-file analysis during indexing
4. **Query Engine**: Hybrid search combining exact name matching, fuzzy search, and semantic embeddings

### Why This Architecture
- **Reuses existing infrastructure**: AST extraction, embeddings, LanceDB storage
- **Minimal overhead**: Symbol data piggybacks on existing chunks
- **Fast queries**: In-memory index for symbol lookups, LanceDB for semantic search
- **Incremental adoption**: Can be added without breaking existing functionality

---

## 2. DDD Design

### Bounded Contexts

#### Symbol Context
**Aggregate Root**: `Symbol`
```typescript
interface Symbol {
  id: SymbolId;           // Composite: file_path + line + name
  definition: Definition;
  references: Reference[];
  metadata: SymbolMetadata;
}
```

**Entities**:
- `Definition`: Where symbol is defined
- `Reference`: Where symbol is used

**Value Objects**:
- `SymbolId`: Unique identifier
- `SymbolKind`: Function, Class, Struct, etc.
- `SymbolSignature`: Type signature
- `SymbolVisibility`: Public, Private, Protected
- `Location`: File path + line range

#### Search Context
**Domain Services**:
- `SymbolIndexer`: Extracts and indexes symbols from AST
- `SymbolSearcher`: Queries symbols by various criteria
- `ReferenceResolver`: Finds symbol references across codebase

**Domain Events**:
- `SymbolIndexed`: New symbol added to index
- `SymbolUpdated`: Symbol definition changed
- `SymbolDeleted`: Symbol removed from codebase

### Aggregate Invariants
1. **Symbol uniqueness**: No duplicate symbols with same ID
2. **Location validity**: Symbol locations must exist in indexed files
3. **Reference consistency**: References must point to valid definitions

---

## 3. Symbol Index Schema

### LanceDB Table Enhancement
Extend existing `chunks` table with symbol-specific columns:

```rust
pub struct EnhancedChunkSchema {
    // Existing fields
    id: String,
    content: String,
    file_path: String,
    start_line: i32,
    end_line: i32,
    language: Option<String>,
    vector: Vec<f32>,
    mtime: i64,

    // New symbol fields
    semantic_kind: Option<String>,    // "function", "class", "struct"
    symbol_name: Option<String>,      // Extracted name
    symbol_signature: Option<String>, // Function signature, type definition
    symbol_parent: Option<String>,    // Parent scope (class, module)
    symbol_visibility: Option<String>,// "public", "private", "protected"
    is_definition: bool,              // True if this is the definition
    symbol_id: Option<String>,        // Unique symbol identifier
}
```

### In-Memory Symbol Index
Built on startup for fast lookups:

```rust
pub struct SymbolIndex {
    // Primary indexes
    by_name: HashMap<String, Vec<SymbolEntry>>,
    by_file: HashMap<PathBuf, Vec<SymbolEntry>>,
    by_kind: HashMap<SemanticKind, Vec<SymbolEntry>>,

    // Reference tracking (lazy loaded)
    references: HashMap<SymbolId, Vec<Location>>,
    definitions: HashMap<SymbolId, Location>,

    // Fuzzy search support
    name_trie: TrieNode,
}

pub struct SymbolEntry {
    pub symbol_id: String,
    pub name: String,
    pub kind: SemanticKind,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub parent: Option<String>,
    pub visibility: Option<String>,
    pub chunk_id: String,  // Reference to chunk in LanceDB
}
```

---

## 4. tRPC Contracts (MCP Tool Definitions)

### Tool 1: find_symbol
Find symbol definitions by name with advanced filtering.

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindSymbolRequest {
    #[schemars(description = "Symbol name or pattern to search for")]
    query: String,

    #[schemars(description = "Filter by semantic kind (function, class, struct, etc.)")]
    kind: Option<String>,

    #[schemars(description = "Filter by programming language")]
    language: Option<String>,

    #[schemars(description = "Filter by file path pattern (glob)")]
    file_pattern: Option<String>,

    #[schemars(description = "Include parent context in results")]
    include_parent: Option<bool>,

    #[schemars(description = "Search mode: 'exact', 'prefix', 'fuzzy', 'semantic'")]
    mode: Option<String>,

    #[schemars(description = "Maximum number of results (default: 20)")]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct FindSymbolResponse {
    pub symbols: Vec<SymbolResult>,
    pub total_matches: usize,
    pub search_mode_used: String,
}

#[derive(Debug, Serialize)]
pub struct SymbolResult {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub parent: Option<String>,
    pub visibility: Option<String>,
    pub documentation: Option<String>,
    pub relevance_score: f32,
}
```

### Tool 2: find_references
Find all references to a specific symbol.

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencesRequest {
    #[schemars(description = "Symbol name to find references for")]
    symbol_name: String,

    #[schemars(description = "File path where symbol is defined (for disambiguation)")]
    definition_file: Option<String>,

    #[schemars(description = "Line number where symbol is defined (for precise matching)")]
    definition_line: Option<usize>,

    #[schemars(description = "Include the definition in results")]
    include_definition: Option<bool>,

    #[schemars(description = "Group results by file")]
    group_by_file: Option<bool>,

    #[schemars(description = "Maximum number of results (default: 50)")]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct FindReferencesResponse {
    pub definition: Option<SymbolResult>,
    pub references: Vec<ReferenceResult>,
    pub total_references: usize,
    pub files_affected: usize,
}

#[derive(Debug, Serialize)]
pub struct ReferenceResult {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub line_content: String,
    pub usage_context: String,  // "call", "import", "type_annotation", etc.
}
```

### Tool 3: list_symbols
List all symbols in a file or matching a pattern.

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSymbolsRequest {
    #[schemars(description = "File path to list symbols from")]
    file_path: Option<String>,

    #[schemars(description = "Filter by symbol kind")]
    kind_filter: Option<Vec<String>>,

    #[schemars(description = "Include only public symbols")]
    public_only: Option<bool>,

    #[schemars(description = "Group symbols by kind")]
    group_by_kind: Option<bool>,

    #[schemars(description = "Sort order: 'alphabetical', 'line_number', 'kind'")]
    sort_by: Option<String>,

    #[schemars(description = "Include symbol hierarchy (nested symbols)")]
    include_hierarchy: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ListSymbolsResponse {
    pub file_path: Option<String>,
    pub symbols: Vec<SymbolSummary>,
    pub total_symbols: usize,
    pub by_kind: Option<HashMap<String, Vec<SymbolSummary>>>,
}

#[derive(Debug, Serialize)]
pub struct SymbolSummary {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub signature: Option<String>,
    pub parent: Option<String>,
    pub children: Option<Vec<String>>,
}
```

---

## 5. Database Schema

### Enhanced LanceDB Schema
```sql
-- Conceptual schema (LanceDB uses Arrow format)
TABLE chunks_v2 (
    -- Existing columns
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    file_path TEXT NOT NULL,
    start_line INT NOT NULL,
    end_line INT NOT NULL,
    language TEXT,
    vector FLOAT32[768] NOT NULL,
    mtime BIGINT NOT NULL,

    -- New symbol columns
    semantic_kind TEXT,         -- SemanticKind enum as string
    symbol_name TEXT,           -- Indexed for fast lookup
    symbol_signature TEXT,      -- Full signature
    symbol_parent TEXT,         -- Parent context
    symbol_visibility TEXT,     -- Visibility modifier
    is_definition BOOLEAN,      -- True if definition, false if reference
    symbol_id TEXT,            -- Composite key for unique identification

    -- Indexes (conceptual - LanceDB handles internally)
    INDEX idx_symbol_name (symbol_name),
    INDEX idx_symbol_kind (semantic_kind),
    INDEX idx_file_path (file_path),
    INDEX idx_symbol_id (symbol_id),
    INDEX idx_vector (vector) -- For ANN search
);
```

### Query Patterns
```rust
// Fast exact symbol lookup
SELECT * FROM chunks_v2
WHERE symbol_name = ? AND is_definition = true;

// Find all symbols in a file
SELECT * FROM chunks_v2
WHERE file_path = ? AND semantic_kind IS NOT NULL
ORDER BY start_line;

// Find references to a symbol
SELECT * FROM chunks_v2
WHERE content LIKE '%symbol_name%'
  AND symbol_id != ?
  AND is_definition = false;

// Semantic search with symbol filter
SELECT * FROM chunks_v2
WHERE semantic_kind = ?
ORDER BY vector <-> ? -- Cosine similarity
LIMIT ?;
```

---

## 6. Integration Patterns

### WebSocket Events
For real-time symbol updates in IDE integrations:

```typescript
interface SymbolEvents {
  'symbol:indexed': {
    file: string;
    symbols: SymbolSummary[];
  };
  'symbol:updated': {
    symbol_id: string;
    changes: Partial<SymbolResult>;
  };
  'symbol:deleted': {
    symbol_id: string;
    file: string;
  };
}
```

### Caching Strategy
Multi-level caching for performance:

```rust
pub struct SymbolCache {
    // L1: Hot symbols (LRU, 100 entries)
    hot_symbols: LruCache<String, SymbolResult>,

    // L2: Recent searches (TTL: 5 minutes)
    search_cache: TtlCache<String, Vec<SymbolResult>>,

    // L3: File symbol lists (invalidated on file change)
    file_symbols: HashMap<PathBuf, (i64, Vec<SymbolSummary>)>,
}
```

### External API Integration
For language servers and IDE plugins:

```rust
// LSP-compatible symbol information
impl From<SymbolResult> for lsp_types::SymbolInformation {
    fn from(symbol: SymbolResult) -> Self {
        lsp_types::SymbolInformation {
            name: symbol.name,
            kind: map_semantic_kind_to_lsp(symbol.kind),
            location: lsp_types::Location {
                uri: file_path_to_uri(symbol.file_path),
                range: lines_to_range(symbol.start_line, symbol.end_line),
            },
            container_name: symbol.parent,
            deprecated: None,
            tags: None,
        }
    }
}
```

---

## 7. Implementation Strategy

### Phase 1: Foundation (Week 1)
1. **Extend chunk schema** with symbol metadata fields
2. **Update AST extractors** to populate symbol fields
3. **Build in-memory index** on server startup
4. **Implement `find_symbol`** tool with exact matching

### Phase 2: Advanced Search (Week 2)
1. **Add fuzzy search** using Levenshtein distance
2. **Implement prefix search** with trie structure
3. **Integrate semantic search** for natural language queries
4. **Add `list_symbols`** tool for file exploration

### Phase 3: References (Week 3)
1. **Simple reference finder** using text search
2. **AST-based reference extraction** for accuracy
3. **Cross-file reference tracking**
4. **Implement `find_references`** tool

### Phase 4: Optimization (Week 4)
1. **Performance profiling** and bottleneck identification
2. **Cache implementation** for frequently accessed symbols
3. **Incremental indexing** for file changes
4. **Batch operations** for multiple symbol queries

---

## 8. Performance Analysis

### Index Size Estimates
For a 10,000 file codebase with ~50 symbols per file:

```
Symbol Count: 500,000 symbols
Index Memory:
  - SymbolEntry: ~200 bytes each
  - HashMap overhead: ~50 bytes per entry
  - Total: ~125 MB in memory

LanceDB Storage:
  - Additional columns: ~100 bytes per chunk
  - 1M chunks: ~100 MB additional storage
```

### Query Performance Targets
```
Operation                 | Target  | Method
--------------------------|---------|-------------------------
Exact symbol lookup       | <10ms   | HashMap O(1)
Prefix search            | <50ms   | Trie traversal
Fuzzy search (single)    | <100ms  | Levenshtein with cutoff
Semantic search          | <200ms  | Vector similarity
List file symbols        | <50ms   | Pre-indexed by file
Find references (simple) | <500ms  | Text search with index
Find references (AST)    | <2s     | AST parsing + analysis
```

### Caching Impact
```
Cache Hit Ratios (estimated):
- Hot symbols (L1): 60% hit rate → 6ms saved per hit
- Search cache (L2): 30% hit rate → 150ms saved per hit
- File symbols (L3): 80% hit rate → 40ms saved per hit

Overall latency reduction: ~40% for typical workflows
```

### Scaling Considerations
1. **Horizontal scaling**: Shard symbol index by file path prefix
2. **Lazy loading**: Load references only when requested
3. **Incremental updates**: Process only changed files
4. **Background indexing**: Non-blocking symbol extraction

---

## 9. Type Definitions

### Core Types
```typescript
// Domain types
type SymbolId = string;  // Format: "file_path:line:name"
type FilePath = string;
type LineNumber = number;

enum SemanticKind {
  Function = "function",
  Method = "method",
  Class = "class",
  Struct = "struct",
  Interface = "interface",
  Enum = "enum",
  Trait = "trait",
  Module = "module",
  Constant = "constant",
  TypeAlias = "type_alias",
  Macro = "macro",
  Test = "test",
}

enum Visibility {
  Public = "public",
  Private = "private",
  Protected = "protected",
  Internal = "internal",
}

enum SearchMode {
  Exact = "exact",
  Prefix = "prefix",
  Fuzzy = "fuzzy",
  Semantic = "semantic",
}

interface Location {
  file_path: FilePath;
  start_line: LineNumber;
  end_line: LineNumber;
  column?: number;
}

interface SymbolMetadata {
  kind: SemanticKind;
  visibility?: Visibility;
  signature?: string;
  parent?: string;
  documentation?: string;
}

interface Symbol {
  id: SymbolId;
  name: string;
  location: Location;
  metadata: SymbolMetadata;
}
```

### API Contracts
```typescript
// MCP Tool Request/Response types
interface MCPToolRequest<T> {
  tool: string;
  parameters: T;
}

interface MCPToolResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
  execution_time_ms: number;
}

// Symbol search types
interface SymbolSearchRequest {
  query: string;
  filters: SymbolFilters;
  options: SearchOptions;
}

interface SymbolFilters {
  kind?: SemanticKind[];
  language?: string[];
  file_pattern?: string;
  visibility?: Visibility[];
}

interface SearchOptions {
  mode?: SearchMode;
  limit?: number;
  include_docs?: boolean;
  include_signature?: boolean;
}
```

---

## 10. Architecture Tradeoffs

### Decision: Enhanced Chunks vs Separate Symbol Table

**Chosen: Enhanced Chunks**

Pros:
- Minimal storage overhead
- Reuses existing embedding infrastructure
- Single source of truth for code content
- Simpler backup and migration

Cons:
- Couples symbol data with chunk data
- May increase chunk size
- Requires schema migration
- Less flexibility for symbol-specific optimizations

**Alternative: Separate Symbol Table**

Pros:
- Clean separation of concerns
- Symbol-specific optimizations possible
- Independent scaling
- No impact on existing chunk queries

Cons:
- Data duplication (content stored twice)
- Complex joins for semantic search
- Synchronization challenges
- Additional storage overhead

### Decision: In-Memory Index vs Database-Only

**Chosen: Hybrid (In-Memory + Database)**

Pros:
- Fast symbol lookups (O(1) HashMap)
- Reduced database load
- Enables complex queries (trie, fuzzy)
- Better user experience

Cons:
- Memory overhead (~125MB for 500k symbols)
- Startup time to build index
- Cache invalidation complexity
- Potential inconsistency

### Decision: Lazy Reference Loading vs Pre-computed

**Chosen: Lazy Loading**

Pros:
- Faster initial indexing
- Lower storage requirements
- Handles large codebases better
- References always up-to-date

Cons:
- Slower first reference query
- Complex reference resolution logic
- May miss some references
- Requires AST parsing on demand

---

## 11. Example Queries and Responses

### Example 1: Find Function Definition
```rust
// Request
FindSymbolRequest {
    query: "processPayment",
    kind: Some("function"),
    mode: Some("exact"),
}

// Response
FindSymbolResponse {
    symbols: vec![
        SymbolResult {
            name: "processPayment",
            kind: "function",
            file_path: "src/payments/processor.rs",
            start_line: 45,
            end_line: 89,
            signature: Some("fn processPayment(amount: f64, currency: Currency) -> Result<PaymentId>"),
            parent: None,
            visibility: Some("public"),
            documentation: Some("Processes a payment transaction..."),
            relevance_score: 1.0,
        }
    ],
    total_matches: 1,
    search_mode_used: "exact",
}
```

### Example 2: Find All References
```rust
// Request
FindReferencesRequest {
    symbol_name: "UserAccount",
    include_definition: true,
    group_by_file: true,
}

// Response
FindReferencesResponse {
    definition: Some(SymbolResult {
        name: "UserAccount",
        kind: "struct",
        file_path: "src/models/user.rs",
        start_line: 10,
        end_line: 25,
        // ...
    }),
    references: vec![
        ReferenceResult {
            file_path: "src/services/auth.rs",
            start_line: 34,
            end_line: 34,
            line_content: "    let account: UserAccount = UserAccount::new(email);",
            usage_context: "type_annotation",
        },
        ReferenceResult {
            file_path: "src/services/auth.rs",
            start_line: 34,
            end_line: 34,
            line_content: "    let account: UserAccount = UserAccount::new(email);",
            usage_context: "constructor_call",
        },
        // ...
    ],
    total_references: 23,
    files_affected: 8,
}
```

### Example 3: List File Symbols
```rust
// Request
ListSymbolsRequest {
    file_path: Some("src/lib.rs"),
    group_by_kind: true,
    sort_by: Some("kind"),
}

// Response
ListSymbolsResponse {
    file_path: Some("src/lib.rs"),
    symbols: vec![/* flat list */],
    total_symbols: 15,
    by_kind: Some(HashMap::from([
        ("function", vec![
            SymbolSummary {
                name: "init",
                kind: "function",
                line: 10,
                signature: Some("pub fn init() -> Result<()>"),
                parent: None,
                children: None,
            },
            // ...
        ]),
        ("struct", vec![/* ... */]),
        ("impl", vec![/* ... */]),
    ])),
}
```

---

## 12. Migration Strategy

### Schema Migration
```sql
-- Step 1: Add new columns with defaults
ALTER TABLE chunks ADD COLUMN semantic_kind TEXT DEFAULT NULL;
ALTER TABLE chunks ADD COLUMN symbol_name TEXT DEFAULT NULL;
ALTER TABLE chunks ADD COLUMN symbol_signature TEXT DEFAULT NULL;
ALTER TABLE chunks ADD COLUMN symbol_parent TEXT DEFAULT NULL;
ALTER TABLE chunks ADD COLUMN symbol_visibility TEXT DEFAULT NULL;
ALTER TABLE chunks ADD COLUMN is_definition BOOLEAN DEFAULT false;
ALTER TABLE chunks ADD COLUMN symbol_id TEXT DEFAULT NULL;

-- Step 2: Backfill from existing SemanticUnit data
UPDATE chunks SET
    semantic_kind = extracted_kind,
    symbol_name = extracted_name,
    symbol_signature = extracted_signature,
    symbol_parent = extracted_parent
WHERE extracted_kind IS NOT NULL;

-- Step 3: Create indexes
CREATE INDEX idx_symbol_name ON chunks(symbol_name);
CREATE INDEX idx_semantic_kind ON chunks(semantic_kind);
CREATE INDEX idx_symbol_id ON chunks(symbol_id);
```

### Rollback Plan
1. Feature flag for symbol search tools
2. Keep old chunk schema compatible
3. Dual-write during transition period
4. Monitor performance metrics
5. Gradual rollout by percentage

---

## 13. Success Metrics

### Performance KPIs
- P50 symbol lookup latency < 20ms
- P95 symbol lookup latency < 100ms
- P99 symbol lookup latency < 500ms
- Index build time < 30s for 10k files
- Memory usage < 200MB for 500k symbols

### Quality Metrics
- Symbol extraction accuracy > 95%
- Reference detection precision > 90%
- Reference detection recall > 85%
- Fuzzy search relevance > 80% user satisfaction

### Adoption Metrics
- Daily active symbol searches
- Unique users using symbol tools
- Symbol tool success rate
- User feedback scores

---

## 14. Future Enhancements

### Near-term (3-6 months)
1. **Type inference**: Extract and index type information
2. **Call graph**: Build function call relationships
3. **Symbol rename**: Refactoring support
4. **Go-to-definition**: IDE-like navigation
5. **Symbol documentation**: Extract and index docstrings

### Long-term (6-12 months)
1. **Cross-repository symbols**: Search across multiple projects
2. **Symbol evolution**: Track symbol changes over time
3. **Dependency graph**: Visualize symbol dependencies
4. **Smart suggestions**: ML-based symbol recommendations
5. **Code intelligence**: Advanced static analysis integration

---

**Document Generated**: 2025-12-06
**Architecture Type**: Symbol Search MCP Tools
**Decision Rationale**: Balances performance, storage efficiency, and implementation complexity while leveraging existing CodeRAG infrastructure