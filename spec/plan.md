# CodeRAG Implementation Plan

## Project Overview

**Stack**: Rust (detected: greenfield project, spec defines Cargo.toml)
**Type**: CLI tool + MCP server for semantic code search
**MVP Scope**: v0.1 features from specification

---

## Phase 0: Project Setup

### 0.1 Initialize Cargo Project
- [ ] Create `Cargo.toml` with all dependencies from spec
- [ ] Create basic project structure directories
- [ ] Setup `.gitignore` for Rust project

### 0.2 Directory Structure
```
coderag/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── cli/
│   │   └── mod.rs
│   ├── indexer/
│   │   ├── mod.rs
│   │   ├── walker.rs
│   │   └── chunker.rs
│   ├── embeddings/
│   │   ├── mod.rs
│   │   └── fastembed.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   └── lancedb.rs
│   ├── search/
│   │   ├── mod.rs
│   │   └── vector.rs
│   └── mcp/
│       ├── mod.rs
│       └── server.rs
└── config/
    └── default.toml
```

---

## Phase 1: CLI Skeleton (clap)

### 1.1 Define CLI Commands
- [ ] `init` - Initialize CodeRAG in a project directory
- [ ] `index` - Index/re-index the codebase
- [ ] `serve` - Start MCP server
- [ ] `search` - CLI search for debugging

### 1.2 Implementation Details
**File**: `src/cli/mod.rs`

```rust
// Commands enum with clap derive
pub enum Commands {
    Init,
    Index,
    Serve,
    Search { query: String, limit: Option<usize> },
}
```

### 1.3 Configuration Loading
**File**: `src/config.rs`
- [ ] Define `Config` struct matching spec TOML format
- [ ] Load from `.coderag/config.toml` if exists
- [ ] Fall back to defaults

---

## Phase 2: File Walker (indexer/walker.rs)

### 2.1 Core Functionality
- [ ] Use `ignore` crate for .gitignore support
- [ ] Filter by configured extensions (rs, py, ts, js, go, java)
- [ ] Apply custom ignore patterns from config
- [ ] Return iterator of file paths

### 2.2 Implementation Details
```rust
pub struct Walker {
    root: PathBuf,
    extensions: Vec<String>,
    ignore_patterns: Vec<String>,
}

impl Walker {
    pub fn new(root: PathBuf, config: &IndexerConfig) -> Self;
    pub fn walk(&self) -> impl Iterator<Item = PathBuf>;
}
```

---

## Phase 3: Chunker (indexer/chunker.rs)

### 3.1 MVP: Line-Based Chunking
- [ ] Split files into chunks of ~512 tokens
- [ ] Preserve context (don't split mid-function if possible)
- [ ] Track file path and line numbers for each chunk

### 3.2 Data Structures
```rust
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
}

pub struct Chunker {
    chunk_size: usize, // in tokens (approximate)
}

impl Chunker {
    pub fn chunk_file(&self, path: &Path, content: &str) -> Vec<Chunk>;
}
```

### 3.3 Token Estimation
- [ ] Use simple heuristic: ~4 chars per token
- [ ] Chunk at natural boundaries (blank lines, function end)

---

## Phase 4: Embeddings (embeddings/fastembed.rs)

### 4.1 fastembed Integration
- [ ] Initialize `TextEmbedding` with model from config
- [ ] Default model: `nomic-embed-text-v1.5`
- [ ] Batch embedding generation

### 4.2 Implementation
```rust
pub struct EmbeddingGenerator {
    model: TextEmbedding,
    batch_size: usize,
}

impl EmbeddingGenerator {
    pub fn new(config: &EmbeddingsConfig) -> Result<Self>;
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    pub fn embed_query(&self, query: &str) -> Result<Vec<f32>>;
}
```

### 4.3 Considerations
- [ ] Handle model download on first run
- [ ] Progress bar for initial model download
- [ ] Batch processing for memory efficiency

---

## Phase 5: Storage (storage/lancedb.rs)

### 5.1 LanceDB Setup
- [ ] Create/open database at `.coderag/index.lance`
- [ ] Define schema for chunks table

### 5.2 Schema
```rust
// Arrow schema for LanceDB
// Columns:
// - id: String (UUID)
// - content: String
// - file_path: String
// - start_line: Int32
// - end_line: Int32
// - language: String (nullable)
// - vector: FixedSizeList<Float32, 768> (embedding dimension)
// - mtime: Int64 (file modification time for incremental indexing)
```

### 5.3 Implementation
```rust
pub struct Storage {
    db: Connection,
    table: Table,
}

impl Storage {
    pub async fn new(path: &Path) -> Result<Self>;
    pub async fn insert_chunks(&self, chunks: Vec<IndexedChunk>) -> Result<()>;
    pub async fn search(&self, vector: Vec<f32>, limit: usize) -> Result<Vec<SearchResult>>;
    pub async fn get_file_mtimes(&self) -> Result<HashMap<PathBuf, i64>>;
    pub async fn delete_by_file(&self, path: &Path) -> Result<()>;
    pub async fn list_files(&self, pattern: Option<&str>) -> Result<Vec<String>>;
}
```

---

## Phase 6: Vector Search (search/vector.rs)

### 6.1 Search Implementation
- [ ] Convert query to embedding
- [ ] Perform ANN search in LanceDB
- [ ] Return ranked results with scores

### 6.2 Implementation
```rust
pub struct SearchEngine {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
}

impl SearchEngine {
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}

#[derive(Debug)]
pub struct SearchResult {
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f32,
}
```

---

## Phase 7: MCP Server (mcp/server.rs)

### 7.1 rmcp Integration
- [ ] Setup stdio transport (default)
- [ ] Implement `search` tool
- [ ] Implement `list_files` tool (v0.2 but simple)
- [ ] Implement `get_file` tool (v0.2 but simple)

### 7.2 Tool Definitions
```rust
// search tool
{
    name: "search",
    description: "Найти релевантные куски кода по запросу",
    parameters: {
        query: String,
        limit: Option<usize>,
    }
}

// list_files tool
{
    name: "list_files",
    description: "Получить список файлов в индексе",
    parameters: {
        pattern: Option<String>,
    }
}

// get_file tool
{
    name: "get_file",
    description: "Получить полное содержимое файла",
    parameters: {
        path: String,
    }
}
```

### 7.3 Server Implementation
```rust
pub struct CodeRagServer {
    search_engine: Arc<SearchEngine>,
    storage: Arc<Storage>,
    root_path: PathBuf,
}

impl CodeRagServer {
    pub async fn run(&self) -> Result<()>;
}
```

---

## Phase 8: Main Integration

### 8.1 init Command
1. Create `.coderag/` directory
2. Create default `config.toml`
3. Print success message

### 8.2 index Command
1. Load config
2. Initialize walker
3. Walk files, chunk them
4. Generate embeddings (with progress bar)
5. Store in LanceDB
6. Print statistics (files indexed, chunks created)

### 8.3 serve Command
1. Load config
2. Initialize storage
3. Initialize embedding generator
4. Start MCP server on stdio

### 8.4 search Command
1. Load config
2. Initialize search engine
3. Perform search
4. Format and print results

---

## Implementation Order (Step-by-Step)

### Step 1: Project Bootstrap
1. Create `Cargo.toml`
2. Create directory structure
3. Create `src/main.rs` with basic clap setup
4. Create `src/lib.rs` with module declarations
5. Verify: `cargo check` passes

### Step 2: Configuration
1. Create `src/config.rs` with Config structs
2. Create `config/default.toml`
3. Implement config loading
4. Verify: Unit tests for config parsing

### Step 3: File Walker
1. Create `src/indexer/mod.rs`
2. Create `src/indexer/walker.rs`
3. Implement Walker with ignore crate
4. Verify: Can walk a test directory

### Step 4: Chunker
1. Create `src/indexer/chunker.rs`
2. Implement line-based chunking
3. Verify: Can chunk sample code files

### Step 5: Embeddings
1. Create `src/embeddings/mod.rs`
2. Create `src/embeddings/fastembed.rs`
3. Implement EmbeddingGenerator
4. Verify: Can generate embeddings for sample text

### Step 6: Storage
1. Create `src/storage/mod.rs`
2. Create `src/storage/lancedb.rs`
3. Implement Storage with LanceDB
4. Verify: Can store and retrieve chunks

### Step 7: Search
1. Create `src/search/mod.rs`
2. Create `src/search/vector.rs`
3. Implement SearchEngine
4. Verify: End-to-end search works

### Step 8: CLI Commands
1. Implement `init` command
2. Implement `index` command with progress
3. Implement `search` command
4. Verify: Full CLI workflow works

### Step 9: MCP Server
1. Create `src/mcp/mod.rs`
2. Create `src/mcp/server.rs`
3. Implement rmcp server with search tool
4. Add list_files and get_file tools
5. Verify: Works with Claude Desktop

### Step 10: Polish & Testing
1. Add error handling throughout
2. Add logging with tracing
3. Write integration tests
4. Create README.md with usage instructions

---

## Dependencies Summary

```toml
[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# CLI
clap = { version = "4", features = ["derive"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# File walking
ignore = "0.4"
walkdir = "2"

# Embeddings
fastembed = "4"

# Vector storage
lancedb = "0.15"
arrow-array = "53"
arrow-schema = "53"

# MCP
rmcp = { version = "0.1", features = ["server", "transport-io"] }

# Utilities
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
indicatif = "0.17"  # Progress bars
glob = "0.3"        # Pattern matching for list_files
```

---

## Testing Strategy

### Unit Tests
- Config parsing
- Chunker logic
- File pattern matching

### Integration Tests
- Full indexing pipeline
- Search accuracy
- MCP tool responses

### Manual Testing
- Test with real codebases
- Test with Claude Desktop
- Test incremental re-indexing

---

## Success Criteria for MVP

1. ✅ `coderag init` creates `.coderag/` directory with config
2. ✅ `coderag index` indexes all code files with progress bar
3. ✅ `coderag search "query"` returns relevant results
4. ✅ `coderag serve` starts MCP server
5. ✅ Claude Desktop can use the MCP server to search code
6. ✅ Indexes common languages: Rust, Python, TypeScript, JavaScript, Go, Java

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| fastembed model download slow | Show progress, cache model |
| LanceDB API changes | Pin version, add abstraction layer |
| rmcp documentation sparse | Explore examples, fallback to JSON-RPC manual impl |
| Large codebases slow | Batch processing, progress bars, incremental indexing |

---

## Post-MVP (v0.2 Features)

- Tree-sitter AST-based chunking
- Incremental re-indexing (mtime tracking)
- Watch mode (fsnotify)
- Better MCP tools

---

## Questions for User

Before starting implementation:

1. **Model choice**: Stick with `nomic-embed-text-v1.5` or allow `all-MiniLM-L6-v2` (faster, smaller)?
2. **Default chunk size**: 512 tokens as specified, or adjust?
3. **rmcp version**: The spec says `rmcp = "0.1"` but this may not exist yet - should we use a different MCP library or implement manually?
