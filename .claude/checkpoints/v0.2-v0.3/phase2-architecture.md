# CodeRAG v0.2 and v0.3 Architecture Specification

## Executive Summary

This document specifies the architecture for CodeRAG v0.2 (AST Chunking, Watch Mode) and v0.3 (Hybrid Search, Multi-project, Metrics, Web UI, HTTP MCP Transport). The design follows Rust idioms with trait-based abstractions, async patterns, and clean module boundaries.

---

## Current Architecture Overview

```
                    +-----------------+
                    |    CLI (clap)   |
                    +--------+--------+
                             |
              +--------------+--------------+
              |              |              |
        +-----v-----+  +-----v-----+  +-----v-----+
        |   init    |  |   index   |  |   serve   |
        +-----------+  +-----+-----+  +-----+-----+
                             |              |
                    +--------v--------+     |
                    |     Walker      |     |
                    +--------+--------+     |
                             |              |
                    +--------v--------+     |
                    |  Chunker (line) |     |
                    +--------+--------+     |
                             |              |
                    +--------v--------+     |
                    | EmbeddingGen    |     |
                    +--------+--------+     |
                             |              |
              +--------------+--------------+
              |                             |
        +-----v-----+                 +-----v-----+
        |  Storage  |<--------------->| SearchEng |
        | (LanceDB) |                 |  (vector) |
        +-----------+                 +-----+-----+
                                            |
                                      +-----v-----+
                                      | MCP Server|
                                      |  (stdio)  |
                                      +-----------+
```

### Current Module Structure

```
src/
  lib.rs                 # Re-exports
  main.rs                # Entry point
  config.rs              # Configuration (Config, IndexerConfig, etc.)
  cli/mod.rs             # CLI parsing
  commands/
    mod.rs
    init.rs              # coderag init
    index.rs             # coderag index
    search.rs            # coderag search
    serve.rs             # coderag serve (MCP)
  indexer/
    mod.rs
    chunker.rs           # Chunk struct, Chunker (line-based)
    walker.rs            # Walker (file discovery)
  embeddings/
    mod.rs
    fastembed.rs         # EmbeddingGenerator
  storage/
    mod.rs
    lancedb.rs           # Storage, IndexedChunk, SearchResult
  search/
    mod.rs
    vector.rs            # SearchEngine
  mcp/
    mod.rs
    server.rs            # CodeRagServer (stdio transport)
```

### Current Key Types

```rust
// indexer/chunker.rs
pub struct Chunk {
    pub content: String,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
}

// storage/lancedb.rs
pub struct IndexedChunk {
    pub id: String,
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
    pub vector: Vec<f32>,
    pub mtime: i64,
}

pub struct SearchResult {
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f32,
}
```

---

## v0.2 Architecture

### Feature 1: Tree-sitter AST Chunking

#### Design Rationale

The current line-based chunker produces chunks that may split semantic units (functions, structs, classes) across boundaries. AST-based chunking extracts complete semantic units, improving search relevance because:

1. **Semantic Coherence**: Each chunk represents a complete, meaningful code unit
2. **Better Embeddings**: Complete functions/classes produce more accurate embeddings
3. **Precise Results**: Search results point to exact definitions, not arbitrary line ranges

#### Module Structure

```
src/indexer/
  mod.rs                      # Add: pub mod ast_chunker;
  chunker.rs                  # Existing line-based chunker (keep as fallback)
  ast_chunker/
    mod.rs                    # AST chunker orchestration
    parser.rs                 # Tree-sitter parsing utilities
    extractors/
      mod.rs                  # Extractor trait + registry
      rust.rs                 # Rust-specific extraction
      python.rs               # Python-specific extraction
      typescript.rs           # TypeScript/JavaScript extraction
      go.rs                   # Go-specific extraction
      java.rs                 # Java-specific extraction
      generic.rs              # Fallback extractor for unknown languages
```

#### Architecture Diagram

```
                           +------------------+
                           |  AstChunker      |
                           |------------------|
                           | - extractors:    |
                           |   ExtractorReg   |
                           | - fallback:      |
                           |   Chunker        |
                           +--------+---------+
                                    |
                +-------------------+-------------------+
                |                                       |
        +-------v-------+                       +-------v-------+
        |    Parser     |                       | ExtractorReg  |
        |---------------|                       |---------------|
        | - parsers:    |                       | - extractors: |
        |   HashMap<    |                       |   HashMap<    |
        |   Lang,Parser>|                       |   Lang,       |
        +-------+-------+                       |   Box<Extrac>>|
                |                               +-------+-------+
                v                                       |
        +---------------+                 +-------------+-------------+
        | tree_sitter:: |                 |             |             |
        | Tree          |           +-----v-----+ +-----v-----+ +-----v-----+
        +---------------+           |RustExtract| |PyExtract  | |TsExtract  |
                                    +-----------+ +-----------+ +-----------+
```

#### Trait Definitions

```rust
// src/indexer/ast_chunker/extractors/mod.rs

/// Represents a semantic code unit extracted from AST
#[derive(Debug, Clone)]
pub struct SemanticUnit {
    /// The type of semantic unit
    pub kind: SemanticKind,
    /// The name/identifier of the unit (if applicable)
    pub name: Option<String>,
    /// The full source code of this unit
    pub content: String,
    /// Documentation/comments associated with this unit
    pub docs: Option<String>,
    /// Start line (1-indexed)
    pub start_line: usize,
    /// End line (1-indexed)
    pub end_line: usize,
    /// Start byte offset in source
    pub start_byte: usize,
    /// End byte offset in source
    pub end_byte: usize,
    /// Signature or type information (for functions, methods)
    pub signature: Option<String>,
    /// Parent context (e.g., class name for methods)
    pub parent: Option<String>,
}

/// Types of semantic units we can extract
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// Fallback for unrecognized but complete blocks
    Block,
}

/// Trait for language-specific AST extractors
pub trait SemanticExtractor: Send + Sync {
    /// Get the tree-sitter language for this extractor
    fn language(&self) -> tree_sitter::Language;

    /// Get the language identifier string (e.g., "rust", "python")
    fn language_id(&self) -> &'static str;

    /// Extract semantic units from a parsed tree
    ///
    /// # Arguments
    /// * `tree` - The parsed tree-sitter Tree
    /// * `source` - The original source code bytes
    ///
    /// # Returns
    /// Vector of extracted semantic units
    fn extract(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<SemanticUnit>;

    /// Get query patterns for this language (optional, for query-based extraction)
    fn queries(&self) -> Option<&str> {
        None
    }
}
```

```rust
// src/indexer/ast_chunker/parser.rs

use std::collections::HashMap;
use anyhow::Result;
use tree_sitter::{Language, Parser, Tree};

/// Manages tree-sitter parsers for multiple languages
pub struct ParserPool {
    parsers: HashMap<String, Parser>,
}

impl ParserPool {
    /// Create a new parser pool with supported languages
    pub fn new() -> Result<Self>;

    /// Parse source code for a given language
    ///
    /// # Arguments
    /// * `language` - Language identifier (e.g., "rust", "python")
    /// * `source` - Source code to parse
    ///
    /// # Returns
    /// Parsed tree or None if language is unsupported
    pub fn parse(&mut self, language: &str, source: &[u8]) -> Option<Tree>;

    /// Check if a language is supported
    pub fn supports(&self, language: &str) -> bool;

    /// Get list of supported languages
    pub fn supported_languages(&self) -> Vec<&str>;
}
```

```rust
// src/indexer/ast_chunker/mod.rs

use std::path::Path;
use anyhow::Result;

use crate::indexer::Chunk;
use super::chunker::Chunker;

/// AST-based chunker that extracts semantic units from code
pub struct AstChunker {
    parser_pool: ParserPool,
    extractors: ExtractorRegistry,
    fallback: Chunker,
    /// Minimum chunk size (units smaller than this get merged)
    min_chunk_tokens: usize,
    /// Maximum chunk size (units larger than this get the fallback)
    max_chunk_tokens: usize,
}

impl AstChunker {
    /// Create a new AST chunker with default configuration
    pub fn new() -> Result<Self>;

    /// Create with custom token limits
    pub fn with_limits(min_tokens: usize, max_tokens: usize) -> Result<Self>;

    /// Chunk a file using AST extraction
    ///
    /// Falls back to line-based chunking if:
    /// - Language is not supported
    /// - Parsing fails
    /// - Extracted units are too large
    ///
    /// # Arguments
    /// * `path` - File path (used for language detection)
    /// * `content` - File content
    ///
    /// # Returns
    /// Vector of chunks, each representing a semantic unit or line-based chunk
    pub fn chunk_file(&mut self, path: &Path, content: &str) -> Vec<Chunk>;

    /// Get statistics about last chunking operation
    pub fn last_stats(&self) -> ChunkingStats;
}

/// Statistics from a chunking operation
#[derive(Debug, Default)]
pub struct ChunkingStats {
    pub method_used: ChunkingMethod,
    pub semantic_units_extracted: usize,
    pub units_merged: usize,
    pub fallback_chunks: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub enum ChunkingMethod {
    #[default]
    LineBased,
    Ast,
    Mixed,
}
```

#### Extractor Registry

```rust
// src/indexer/ast_chunker/extractors/mod.rs

/// Registry of language-specific extractors
pub struct ExtractorRegistry {
    extractors: HashMap<String, Box<dyn SemanticExtractor>>,
}

impl ExtractorRegistry {
    /// Create registry with all built-in extractors
    pub fn new() -> Self;

    /// Get extractor for a language
    pub fn get(&self, language: &str) -> Option<&dyn SemanticExtractor>;

    /// Register a custom extractor
    pub fn register(&mut self, extractor: Box<dyn SemanticExtractor>);
}
```

#### Language-Specific Extractors

Each extractor implements `SemanticExtractor` and knows how to navigate that language's AST:

```rust
// src/indexer/ast_chunker/extractors/rust.rs

pub struct RustExtractor;

impl SemanticExtractor for RustExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn language_id(&self) -> &'static str {
        "rust"
    }

    fn extract(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<SemanticUnit> {
        // Extracts: fn, pub fn, async fn, struct, enum, trait, impl, mod, const, type, macro_rules!
        // Handles: doc comments (///, //!), attributes (#[...])
        // Special handling: #[test] functions marked as SemanticKind::Test
    }

    fn queries(&self) -> Option<&str> {
        Some(r#"
            (function_item) @function
            (struct_item) @struct
            (enum_item) @enum
            (trait_item) @trait
            (impl_item) @impl
            (mod_item) @module
            (const_item) @constant
            (type_item) @type_alias
            (macro_definition) @macro
        "#)
    }
}
```

```rust
// src/indexer/ast_chunker/extractors/python.rs

pub struct PythonExtractor;

impl SemanticExtractor for PythonExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn language_id(&self) -> &'static str {
        "python"
    }

    fn extract(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<SemanticUnit> {
        // Extracts: def, async def, class, decorated functions/classes
        // Handles: docstrings ("""..."""), decorators (@...)
        // Special handling: test_* functions, pytest fixtures
    }
}
```

```rust
// src/indexer/ast_chunker/extractors/typescript.rs

pub struct TypeScriptExtractor;

impl SemanticExtractor for TypeScriptExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn language_id(&self) -> &'static str {
        "typescript"
    }

    fn extract(&self, tree: &tree_sitter::Tree, source: &[u8]) -> Vec<SemanticUnit> {
        // Extracts: function, arrow function, class, interface, type, enum
        // Handles: JSDoc comments, export declarations
        // Also handles: React components (function returning JSX)
    }
}
```

#### Extended Chunk Type

```rust
// src/indexer/chunker.rs - Extended Chunk

#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub file_path: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
    // New fields for v0.2
    /// Type of semantic unit (None for line-based chunks)
    pub semantic_kind: Option<SemanticKind>,
    /// Name of the unit (function name, class name, etc.)
    pub name: Option<String>,
    /// Signature for functions/methods
    pub signature: Option<String>,
    /// Parent context (class name for methods)
    pub parent: Option<String>,
}
```

#### Extended IndexedChunk

```rust
// src/storage/lancedb.rs - Extended IndexedChunk

pub struct IndexedChunk {
    pub id: String,
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
    pub vector: Vec<f32>,
    pub mtime: i64,
    // New fields for v0.2
    /// Semantic kind as string (e.g., "function", "class")
    pub semantic_kind: Option<String>,
    /// Name identifier
    pub name: Option<String>,
    /// Function/method signature
    pub signature: Option<String>,
    /// Parent context
    pub parent: Option<String>,
}
```

#### Configuration Additions

```rust
// src/config.rs - Extended IndexerConfig

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    pub extensions: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub chunk_size: usize,
    // New for v0.2
    /// Chunking strategy: "ast" (default), "line", "hybrid"
    #[serde(default = "default_chunking_strategy")]
    pub chunking_strategy: String,
    /// Minimum tokens for a semantic unit (smaller units get merged)
    #[serde(default = "default_min_chunk_tokens")]
    pub min_chunk_tokens: usize,
    /// Maximum tokens for a semantic unit (larger units use line chunking)
    #[serde(default = "default_max_chunk_tokens")]
    pub max_chunk_tokens: usize,
}

fn default_chunking_strategy() -> String {
    "ast".to_string()
}

fn default_min_chunk_tokens() -> usize {
    50
}

fn default_max_chunk_tokens() -> usize {
    1500
}
```

#### Data Flow: AST Chunking Pipeline

```
File Read
    |
    v
+-------------------+
| Detect Language   |  (from file extension)
+--------+----------+
         |
         v
+-------------------+     Unsupported
| Get Extractor     |------------------+
+--------+----------+                  |
         | Supported                   |
         v                             |
+-------------------+     Parse Error  |
| Parse with        |------------------+
| Tree-sitter       |                  |
+--------+----------+                  |
         | Success                     |
         v                             |
+-------------------+                  |
| Extract Semantic  |                  |
| Units             |                  |
+--------+----------+                  |
         |                             |
         v                             |
+-------------------+                  |
| Post-process:     |                  |
| - Merge small     |                  |
| - Split large     |                  |
| - Add context     |                  |
+--------+----------+                  |
         |                             |
         +<----------------------------+
         |
         v
+-------------------+
| Convert to Chunks |
+--------+----------+
         |
         v
    Vec<Chunk>
```

---

### Feature 2: Watch Mode

#### Design Rationale

Watch mode enables automatic re-indexing when files change. This improves developer experience by keeping the index synchronized without manual intervention. Key considerations:

1. **Debouncing**: Many editors trigger multiple events per save; debounce to avoid redundant work
2. **Batching**: Group changes over a short window to batch embedding generation
3. **Cancellation**: In-flight indexing should be cancellable if more changes arrive
4. **Graceful Shutdown**: Clean shutdown on SIGINT/SIGTERM

#### Module Structure

```
src/
  watcher/
    mod.rs                # Watcher orchestration
    events.rs             # Event types and debouncing
    handler.rs            # Change handling logic
```

#### Architecture Diagram

```
              +------------------+
              |   File System    |
              +--------+---------+
                       |
                       | notify events
                       v
              +------------------+
              |  EventDebouncer  |
              |------------------|
              | - pending:       |
              |   HashMap<Path,  |
              |   ChangeType>    |
              | - delay: 500ms   |
              +--------+---------+
                       |
                       | debounced batch
                       v
              +------------------+
              |  ChangeHandler   |
              |------------------|
              | - indexer        |
              | - storage        |
              | - embedder       |
              +--------+---------+
                       |
                       | (re-index affected files)
                       v
              +------------------+
              |     Storage      |
              +------------------+
```

#### Trait and Struct Definitions

```rust
// src/watcher/events.rs

use std::path::PathBuf;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Types of file system changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// A debounced file change event
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub timestamp: Instant,
}

/// Debounces rapid file system events
pub struct EventDebouncer {
    pending: HashMap<PathBuf, (ChangeType, Instant)>,
    delay: Duration,
}

impl EventDebouncer {
    /// Create a debouncer with the given delay
    pub fn new(delay: Duration) -> Self;

    /// Add an event to the debouncer
    ///
    /// Returns true if this is a new path, false if updating existing
    pub fn add(&mut self, path: PathBuf, change_type: ChangeType) -> bool;

    /// Get all events that have stabilized (older than delay)
    ///
    /// Returns events and removes them from pending
    pub fn drain_ready(&mut self) -> Vec<FileChange>;

    /// Get the duration until the next event is ready (for select timeout)
    pub fn next_ready_in(&self) -> Option<Duration>;

    /// Clear all pending events
    pub fn clear(&mut self);
}
```

```rust
// src/watcher/handler.rs

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::embeddings::EmbeddingGenerator;
use crate::indexer::AstChunker;
use crate::storage::Storage;
use super::events::FileChange;

/// Handles file changes and triggers re-indexing
pub struct ChangeHandler {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    chunker: AstChunker,
    root: PathBuf,
}

impl ChangeHandler {
    /// Create a new change handler
    pub fn new(
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        root: PathBuf,
    ) -> Result<Self>;

    /// Process a batch of file changes
    ///
    /// # Arguments
    /// * `changes` - Batch of debounced file changes
    ///
    /// # Returns
    /// Statistics about the processing
    pub async fn process_changes(&mut self, changes: Vec<FileChange>) -> Result<ProcessingStats>;

    /// Process a single file change
    async fn process_single(&mut self, change: FileChange) -> Result<()>;
}

/// Statistics from change processing
#[derive(Debug, Default)]
pub struct ProcessingStats {
    pub files_added: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
    pub chunks_created: usize,
    pub chunks_removed: usize,
    pub errors: usize,
}
```

```rust
// src/watcher/mod.rs

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::storage::Storage;

pub use events::{ChangeType, FileChange};
pub use handler::{ChangeHandler, ProcessingStats};

mod events;
mod handler;

/// File system watcher for automatic re-indexing
pub struct Watcher {
    root: PathBuf,
    config: WatcherConfig,
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
}

/// Configuration for the watcher
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce delay in milliseconds
    pub debounce_ms: u64,
    /// Whether to process initial scan
    pub initial_scan: bool,
    /// File extensions to watch (empty = use indexer config)
    pub extensions: Vec<String>,
    /// Patterns to ignore
    pub ignore_patterns: Vec<String>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 500,
            initial_scan: false,
            extensions: vec![],
            ignore_patterns: vec![],
        }
    }
}

impl Watcher {
    /// Create a new watcher
    pub fn new(
        root: PathBuf,
        config: WatcherConfig,
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
    ) -> Self;

    /// Start watching for file changes
    ///
    /// This runs until the shutdown signal is received.
    ///
    /// # Arguments
    /// * `shutdown` - Receiver for shutdown signal
    ///
    /// # Returns
    /// Total processing statistics
    pub async fn run(self, mut shutdown: oneshot::Receiver<()>) -> Result<ProcessingStats>;

    /// Start watching and return a handle for control
    pub fn spawn(self) -> WatcherHandle;
}

/// Handle to control a running watcher
pub struct WatcherHandle {
    shutdown_tx: oneshot::Sender<()>,
    stats_rx: mpsc::Receiver<ProcessingStats>,
}

impl WatcherHandle {
    /// Request graceful shutdown
    pub fn shutdown(self);

    /// Receive processing statistics (after shutdown)
    pub async fn stats(mut self) -> ProcessingStats;
}
```

#### CLI Integration

```rust
// src/commands/watch.rs (new file)

use anyhow::{bail, Result};
use std::env;
use std::sync::Arc;
use tokio::signal;

use crate::embeddings::EmbeddingGenerator;
use crate::storage::Storage;
use crate::watcher::{Watcher, WatcherConfig};
use crate::Config;

/// Run the watch command
pub async fn run() -> Result<()> {
    let root = env::current_dir()?;

    if !Config::is_initialized(&root) {
        bail!("CodeRAG is not initialized. Run 'coderag init' first.");
    }

    let config = Config::load(&root)?;

    println!("Starting watch mode...");
    println!("Press Ctrl+C to stop.");

    let storage = Arc::new(Storage::new(&config.db_path(&root)).await?);
    let embedder = Arc::new(EmbeddingGenerator::new(&config.embeddings)?);

    let watcher_config = WatcherConfig {
        debounce_ms: config.watcher.debounce_ms,
        extensions: config.indexer.extensions.clone(),
        ignore_patterns: config.indexer.ignore_patterns.clone(),
        ..Default::default()
    };

    let watcher = Watcher::new(root, watcher_config, storage, embedder);
    let handle = watcher.spawn();

    // Wait for Ctrl+C
    signal::ctrl_c().await?;

    println!("\nShutting down...");
    handle.shutdown();

    let stats = handle.stats().await;
    println!("Watch session complete:");
    println!("  Files added: {}", stats.files_added);
    println!("  Files modified: {}", stats.files_modified);
    println!("  Files deleted: {}", stats.files_deleted);

    Ok(())
}
```

#### Configuration Additions

```rust
// src/config.rs - Add WatcherConfig

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub indexer: IndexerConfig,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub server: ServerConfig,
    // New for v0.2
    #[serde(default)]
    pub watcher: WatcherConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    /// Debounce delay in milliseconds
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
        }
    }
}

fn default_debounce_ms() -> u64 {
    500
}
```

#### Data Flow: Watch Mode Event Handling

```
File System Event (notify)
    |
    v
+----------------------+
| Filter:              |
| - Is file?           |
| - Matches extension? |
| - Not ignored?       |
+----------+-----------+
           | Pass
           v
+----------------------+
| EventDebouncer       |
| - Collect events     |
| - Wait for stable    |
+----------+-----------+
           | After 500ms of no changes
           v
+----------------------+
| Batch Changes        |
| - Group by type      |
+----------+-----------+
           |
           v
+----------------------+
| ChangeHandler        |
|----------------------|
| For Created/Modified:|
| 1. Delete old chunks |
| 2. Read file         |
| 3. AST chunk         |
| 4. Generate embed    |
| 5. Insert chunks     |
|----------------------|
| For Deleted:         |
| 1. Delete chunks     |
+----------+-----------+
           |
           v
+----------------------+
| Update Stats         |
| Print Progress       |
+----------------------+
```

---

## v0.3 Architecture

### Feature 3: Hybrid Search (Vector + BM25)

#### Design Rationale

Vector search excels at semantic similarity but can miss exact keyword matches. BM25 keyword search catches exact terms but lacks semantic understanding. Hybrid search combines both using Reciprocal Rank Fusion (RRF) for best-of-both-worlds results.

#### Module Structure

```
src/search/
  mod.rs                  # Re-exports
  vector.rs               # Existing SearchEngine (rename to VectorSearch)
  bm25/
    mod.rs                # Tantivy BM25 search
    index.rs              # Tantivy index management
    schema.rs             # Tantivy schema definition
  hybrid.rs               # Hybrid search with RRF fusion
  fusion.rs               # Score fusion algorithms
```

#### Architecture Diagram

```
                    +-------------------+
                    |   HybridSearch    |
                    |-------------------|
                    | - vector: Vector  |
                    | - bm25: Bm25      |
                    | - fusion: RRF     |
                    +--------+----------+
                             |
            +----------------+----------------+
            |                                 |
    +-------v-------+                 +-------v-------+
    | VectorSearch  |                 |   Bm25Search  |
    |---------------|                 |---------------|
    | - storage     |                 | - index       |
    | - embedder    |                 | - schema      |
    +-------+-------+                 +-------+-------+
            |                                 |
            v                                 v
    +---------------+                 +---------------+
    |   LanceDB     |                 |    Tantivy    |
    +---------------+                 +---------------+
            |                                 |
            +----------------+----------------+
                             |
                             v
                    +-------------------+
                    |  RRF Fusion       |
                    |-------------------|
                    | Combine rankings  |
                    | Score: 1/(k+rank) |
                    +-------------------+
                             |
                             v
                    Vec<SearchResult>
```

#### Trait and Struct Definitions

```rust
// src/search/mod.rs - Search trait

use anyhow::Result;
use async_trait::async_trait;

/// Common trait for all search implementations
#[async_trait]
pub trait Search: Send + Sync {
    /// Search for relevant chunks
    ///
    /// # Arguments
    /// * `query` - Search query
    /// * `limit` - Maximum results to return
    ///
    /// # Returns
    /// Ranked search results
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    /// Get the search type identifier
    fn search_type(&self) -> &'static str;
}
```

```rust
// src/search/bm25/schema.rs

use tantivy::schema::{Schema, Field, TEXT, STORED, FAST};

/// Schema for the BM25 search index
pub struct Bm25Schema {
    pub schema: Schema,
    pub id: Field,
    pub content: Field,
    pub file_path: Field,
    pub start_line: Field,
    pub end_line: Field,
    pub name: Field,        // Function/class name for boosting
    pub signature: Field,   // For method signature matching
}

impl Bm25Schema {
    pub fn new() -> Self;
}
```

```rust
// src/search/bm25/index.rs

use anyhow::Result;
use std::path::Path;
use tantivy::{Index, IndexWriter, IndexReader};

use super::schema::Bm25Schema;
use crate::storage::IndexedChunk;

/// Manages the Tantivy BM25 index
pub struct Bm25Index {
    index: Index,
    schema: Bm25Schema,
    writer: IndexWriter,
    reader: IndexReader,
}

impl Bm25Index {
    /// Create or open a BM25 index at the given path
    pub fn new(path: &Path) -> Result<Self>;

    /// Add chunks to the index
    pub fn add_chunks(&mut self, chunks: &[IndexedChunk]) -> Result<()>;

    /// Delete chunks by file path
    pub fn delete_by_file(&mut self, path: &str) -> Result<()>;

    /// Commit pending changes
    pub fn commit(&mut self) -> Result<()>;

    /// Clear the entire index
    pub fn clear(&mut self) -> Result<()>;
}
```

```rust
// src/search/bm25/mod.rs

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::search::{Search, SearchResult};

pub use index::Bm25Index;
pub use schema::Bm25Schema;

mod index;
mod schema;

/// BM25 keyword search using Tantivy
pub struct Bm25Search {
    index: Arc<RwLock<Bm25Index>>,
}

impl Bm25Search {
    /// Create a new BM25 search engine
    pub fn new(path: &Path) -> Result<Self>;

    /// Get mutable access to the index for updates
    pub async fn index_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, Bm25Index>;
}

#[async_trait]
impl Search for Bm25Search {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    fn search_type(&self) -> &'static str {
        "bm25"
    }
}
```

```rust
// src/search/fusion.rs

use crate::search::SearchResult;

/// Score fusion algorithms for combining search results
pub trait ScoreFusion: Send + Sync {
    /// Fuse multiple ranked result lists into one
    ///
    /// # Arguments
    /// * `results` - Vector of (result_list, weight) pairs
    /// * `limit` - Maximum results to return
    fn fuse(&self, results: Vec<(Vec<SearchResult>, f32)>, limit: usize) -> Vec<SearchResult>;
}

/// Reciprocal Rank Fusion (RRF)
///
/// Combines rankings using: score = sum(weight / (k + rank))
/// where k is a constant (default 60) and rank is 1-indexed position
pub struct RrfFusion {
    /// Constant k in RRF formula (higher = smoother rank influence)
    k: f32,
}

impl RrfFusion {
    /// Create RRF with default k=60
    pub fn new() -> Self;

    /// Create RRF with custom k value
    pub fn with_k(k: f32) -> Self;
}

impl ScoreFusion for RrfFusion {
    fn fuse(&self, results: Vec<(Vec<SearchResult>, f32)>, limit: usize) -> Vec<SearchResult>;
}

/// Linear score combination
///
/// Normalizes scores and combines: final = sum(weight * normalized_score)
pub struct LinearFusion;

impl ScoreFusion for LinearFusion {
    fn fuse(&self, results: Vec<(Vec<SearchResult>, f32)>, limit: usize) -> Vec<SearchResult>;
}
```

```rust
// src/search/hybrid.rs

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use crate::embeddings::EmbeddingGenerator;
use crate::storage::Storage;
use super::{Search, SearchResult};
use super::bm25::Bm25Search;
use super::fusion::{ScoreFusion, RrfFusion};

/// Hybrid search combining vector similarity and BM25 keyword matching
pub struct HybridSearch {
    vector: VectorSearch,
    bm25: Bm25Search,
    fusion: Box<dyn ScoreFusion>,
    /// Weight for vector results (0.0 - 1.0)
    vector_weight: f32,
    /// Weight for BM25 results (0.0 - 1.0)
    bm25_weight: f32,
}

impl HybridSearch {
    /// Create a new hybrid search engine
    ///
    /// # Arguments
    /// * `storage` - Vector storage
    /// * `embedder` - Embedding generator
    /// * `bm25_path` - Path for Tantivy index
    /// * `vector_weight` - Weight for vector results (default 0.7)
    /// * `bm25_weight` - Weight for BM25 results (default 0.3)
    pub fn new(
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        bm25_path: &Path,
        vector_weight: f32,
        bm25_weight: f32,
    ) -> Result<Self>;

    /// Use custom fusion algorithm
    pub fn with_fusion(mut self, fusion: Box<dyn ScoreFusion>) -> Self;

    /// Get access to the BM25 index for updates
    pub fn bm25(&self) -> &Bm25Search;
}

#[async_trait]
impl Search for HybridSearch {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // 1. Run vector search
        // 2. Run BM25 search (in parallel)
        // 3. Fuse results with RRF
    }

    fn search_type(&self) -> &'static str {
        "hybrid"
    }
}
```

#### Configuration Additions

```rust
// src/config.rs - Search configuration

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Search mode: "vector", "bm25", "hybrid" (default)
    #[serde(default = "default_search_mode")]
    pub mode: String,
    /// Vector weight for hybrid search (0.0-1.0)
    #[serde(default = "default_vector_weight")]
    pub vector_weight: f32,
    /// BM25 weight for hybrid search (0.0-1.0)
    #[serde(default = "default_bm25_weight")]
    pub bm25_weight: f32,
    /// RRF k constant
    #[serde(default = "default_rrf_k")]
    pub rrf_k: f32,
}

fn default_search_mode() -> String { "hybrid".to_string() }
fn default_vector_weight() -> f32 { 0.7 }
fn default_bm25_weight() -> f32 { 0.3 }
fn default_rrf_k() -> f32 { 60.0 }
```

#### Data Flow: Hybrid Search

```
Query: "authentication middleware"
    |
    +---------------------------+
    |                           |
    v                           v
+-------------+           +-------------+
| Vector      |           | BM25        |
| Search      |           | Search      |
+------+------+           +------+------+
       |                         |
       v                         v
[R1, R2, R3...]           [R4, R5, R6...]
(semantic matches)        (keyword matches)
       |                         |
       +------------+------------+
                    |
                    v
           +----------------+
           | RRF Fusion     |
           |----------------|
           | For each doc:  |
           | score = sum(   |
           |   w/(k+rank)   |
           | )              |
           +-------+--------+
                   |
                   v
           [R2, R4, R1, R5...]
           (fused ranking)
```

---

### Feature 4: Multi-project Support

#### Design Rationale

Users often work with multiple projects. A global registry allows:
1. Quick switching between project indices
2. Cross-project search capabilities
3. Centralized configuration management

#### Module Structure

```
src/
  registry/
    mod.rs                # Project registry
    project.rs            # Project metadata
    global.rs             # Global registry file management
```

#### Architecture Diagram

```
~/.coderag/
    registry.json         # Global project registry
    |
    +-- projects/
        +-- <project-hash>/
            +-- index.lance/    # Vector DB
            +-- bm25/           # Tantivy index
            +-- config.toml     # Project config copy

Each Project Directory:
.coderag/
    config.toml           # Local config
    index.lance/          # Vector DB (optional, can use global)
```

#### Struct Definitions

```rust
// src/registry/project.rs

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Metadata about a registered project
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Unique project identifier (hash of canonical path)
    pub id: String,
    /// Human-readable project name
    pub name: String,
    /// Canonical path to project root
    pub path: PathBuf,
    /// When the project was first registered
    pub created_at: DateTime<Utc>,
    /// When the project was last indexed
    pub last_indexed: Option<DateTime<Utc>>,
    /// Number of chunks in index
    pub chunk_count: usize,
    /// Number of files in index
    pub file_count: usize,
    /// Storage location: "local" or "global"
    pub storage_mode: StorageMode,
    /// Project-specific tags for organization
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StorageMode {
    /// Index stored in project's .coderag/
    Local,
    /// Index stored in ~/.coderag/projects/<id>/
    Global,
}

impl ProjectInfo {
    /// Generate project ID from path
    pub fn generate_id(path: &Path) -> String;
}
```

```rust
// src/registry/global.rs

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::project::ProjectInfo;

/// Global project registry stored in ~/.coderag/
pub struct GlobalRegistry {
    registry_path: PathBuf,
    projects: Vec<ProjectInfo>,
}

impl GlobalRegistry {
    /// Load or create the global registry
    pub fn load() -> Result<Self>;

    /// Get the global coderag directory path
    pub fn global_dir() -> Result<PathBuf>;

    /// Register a new project
    pub fn register(&mut self, project: ProjectInfo) -> Result<()>;

    /// Unregister a project by ID
    pub fn unregister(&mut self, id: &str) -> Result<Option<ProjectInfo>>;

    /// Get project by ID
    pub fn get(&self, id: &str) -> Option<&ProjectInfo>;

    /// Get project by path
    pub fn get_by_path(&self, path: &Path) -> Option<&ProjectInfo>;

    /// List all registered projects
    pub fn list(&self) -> &[ProjectInfo];

    /// Search projects by name or tag
    pub fn search(&self, query: &str) -> Vec<&ProjectInfo>;

    /// Update project info
    pub fn update(&mut self, id: &str, f: impl FnOnce(&mut ProjectInfo)) -> Result<bool>;

    /// Save registry to disk
    pub fn save(&self) -> Result<()>;

    /// Get the storage path for a project
    pub fn project_storage_path(&self, id: &str) -> PathBuf;
}
```

```rust
// src/registry/mod.rs

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::storage::Storage;
use crate::search::HybridSearch;

pub use global::GlobalRegistry;
pub use project::{ProjectInfo, StorageMode};

mod global;
mod project;

/// Project manager for multi-project support
pub struct ProjectManager {
    registry: GlobalRegistry,
}

impl ProjectManager {
    /// Create or load the project manager
    pub fn new() -> Result<Self>;

    /// Initialize a project (register if not exists)
    pub async fn init_project(&mut self, path: &Path, name: Option<String>) -> Result<ProjectInfo>;

    /// Get the active project for a path
    pub fn get_project(&self, path: &Path) -> Option<&ProjectInfo>;

    /// Open storage for a project
    pub async fn open_storage(&self, project: &ProjectInfo) -> Result<Arc<Storage>>;

    /// Open search engine for a project
    pub async fn open_search(&self, project: &ProjectInfo) -> Result<HybridSearch>;

    /// List all projects
    pub fn list_projects(&self) -> &[ProjectInfo];

    /// Switch to a different project by ID
    pub fn switch_project(&self, id: &str) -> Result<&ProjectInfo>;
}
```

#### CLI Extensions

```rust
// New commands for multi-project

/// List all registered projects
/// coderag projects list
pub async fn list_projects() -> Result<()>;

/// Register current directory as a project
/// coderag projects add [--name NAME] [--global]
pub async fn add_project(name: Option<String>, global: bool) -> Result<()>;

/// Remove a project from registry
/// coderag projects remove <ID>
pub async fn remove_project(id: &str) -> Result<()>;

/// Switch to a project for subsequent commands
/// coderag projects switch <ID>
pub async fn switch_project(id: &str) -> Result<()>;

/// Search across all projects
/// coderag search-all <QUERY>
pub async fn search_all(query: &str, limit: usize) -> Result<()>;
```

---

### Feature 5: Prometheus Metrics

#### Design Rationale

Observability is critical for production deployments. Prometheus metrics provide:
1. Search latency monitoring
2. Index size and health tracking
3. Resource usage visibility
4. SLA compliance verification

#### Module Structure

```
src/
  metrics/
    mod.rs                # Metrics registry and export
    search.rs             # Search-related metrics
    index.rs              # Indexing metrics
    system.rs             # System resource metrics
```

#### Metrics Definitions

```rust
// src/metrics/mod.rs

use prometheus::{Registry, Counter, Histogram, Gauge, IntGauge};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // Search metrics
    pub static ref SEARCH_REQUESTS_TOTAL: Counter = Counter::new(
        "coderag_search_requests_total",
        "Total number of search requests"
    ).unwrap();

    pub static ref SEARCH_LATENCY_SECONDS: Histogram = Histogram::with_opts(
        histogram_opts!(
            "coderag_search_latency_seconds",
            "Search request latency in seconds",
            vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5]
        )
    ).unwrap();

    pub static ref SEARCH_RESULTS_COUNT: Histogram = Histogram::with_opts(
        histogram_opts!(
            "coderag_search_results_count",
            "Number of results returned per search",
            vec![0.0, 1.0, 5.0, 10.0, 20.0, 50.0, 100.0]
        )
    ).unwrap();

    // Index metrics
    pub static ref INDEX_CHUNKS_TOTAL: IntGauge = IntGauge::new(
        "coderag_index_chunks_total",
        "Total number of chunks in the index"
    ).unwrap();

    pub static ref INDEX_FILES_TOTAL: IntGauge = IntGauge::new(
        "coderag_index_files_total",
        "Total number of files in the index"
    ).unwrap();

    pub static ref INDEXING_DURATION_SECONDS: Histogram = Histogram::with_opts(
        histogram_opts!(
            "coderag_indexing_duration_seconds",
            "Time taken to index files",
            vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 120.0]
        )
    ).unwrap();

    // Embedding metrics
    pub static ref EMBEDDING_REQUESTS_TOTAL: Counter = Counter::new(
        "coderag_embedding_requests_total",
        "Total embedding generation requests"
    ).unwrap();

    pub static ref EMBEDDING_LATENCY_SECONDS: Histogram = Histogram::with_opts(
        histogram_opts!(
            "coderag_embedding_latency_seconds",
            "Embedding generation latency",
            vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0]
        )
    ).unwrap();
}

/// Initialize and register all metrics
pub fn init() {
    REGISTRY.register(Box::new(SEARCH_REQUESTS_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(SEARCH_LATENCY_SECONDS.clone())).unwrap();
    // ... register all metrics
}

/// Export metrics in Prometheus format
pub fn export() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
```

#### Metric Labels

```rust
// src/metrics/search.rs

use prometheus::{HistogramVec, CounterVec};

lazy_static! {
    pub static ref SEARCH_LATENCY_BY_TYPE: HistogramVec = HistogramVec::new(
        histogram_opts!(
            "coderag_search_latency_by_type_seconds",
            "Search latency by search type"
        ),
        &["search_type"]  // "vector", "bm25", "hybrid"
    ).unwrap();

    pub static ref SEARCH_ERRORS_TOTAL: CounterVec = CounterVec::new(
        opts!("coderag_search_errors_total", "Search errors by type"),
        &["error_type"]  // "embedding_failed", "storage_error", etc.
    ).unwrap();
}
```

---

### Feature 6: Web UI (Debug Interface)

#### Design Rationale

A web UI provides:
1. Visual search interface for debugging
2. Index inspection and statistics
3. Real-time watch mode status
4. Configuration management

#### Module Structure

```
src/
  web/
    mod.rs                # Axum app setup
    routes/
      mod.rs              # Route definitions
      search.rs           # Search API endpoints
      index.rs            # Index management endpoints
      metrics.rs          # Metrics endpoint
      status.rs           # Health and status endpoints
    static/
      index.html          # SPA entry point
      app.js              # Frontend JavaScript
      style.css           # Styles
```

#### Architecture Diagram

```
+------------------+
|   Browser        |
+--------+---------+
         |
         | HTTP
         v
+------------------+
|   Axum Server    |
|------------------|
| Routes:          |
| GET  /           | -> Static HTML
| GET  /api/search | -> Search results
| GET  /api/stats  | -> Index stats
| GET  /metrics    | -> Prometheus
| POST /api/reindex| -> Trigger reindex
| WS   /api/watch  | -> Watch status
+--------+---------+
         |
         v
+------------------+
|   Search/Storage |
+------------------+
```

#### Struct Definitions

```rust
// src/web/mod.rs

use axum::{Router, Extension};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::search::HybridSearch;
use crate::storage::Storage;

mod routes;

/// Web server configuration
#[derive(Debug, Clone)]
pub struct WebConfig {
    /// Address to bind to
    pub addr: SocketAddr,
    /// Enable CORS for development
    pub cors: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:9090".parse().unwrap(),
            cors: true,
        }
    }
}

/// Web server state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub search: Arc<HybridSearch>,
    pub storage: Arc<Storage>,
    pub root_path: PathBuf,
}

/// Create the web application router
pub fn create_app(state: AppState) -> Router {
    Router::new()
        .merge(routes::api_routes())
        .merge(routes::static_routes())
        .layer(Extension(state))
        .layer(CorsLayer::permissive())
}

/// Run the web server
pub async fn run(config: WebConfig, state: AppState) -> anyhow::Result<()> {
    let app = create_app(state);
    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

```rust
// src/web/routes/search.rs

use axum::{Json, Extension, extract::Query};
use serde::{Deserialize, Serialize};

use crate::web::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub mode: Option<String>,  // "vector", "bm25", "hybrid"
}

fn default_limit() -> usize { 10 }

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResultDto>,
    pub took_ms: u64,
    pub mode: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResultDto {
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub score: f32,
    pub name: Option<String>,
    pub semantic_kind: Option<String>,
}

/// GET /api/search?q=query&limit=10
pub async fn search(
    Extension(state): Extension<AppState>,
    Query(params): Query<SearchQuery>,
) -> Json<SearchResponse>;
```

```rust
// src/web/routes/status.rs

use axum::{Json, Extension};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub index: IndexStatus,
    pub search: SearchStatus,
}

#[derive(Debug, Serialize)]
pub struct IndexStatus {
    pub total_chunks: usize,
    pub total_files: usize,
    pub last_indexed: Option<String>,
    pub storage_size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct SearchStatus {
    pub mode: String,
    pub vector_enabled: bool,
    pub bm25_enabled: bool,
}

/// GET /api/status
pub async fn status(Extension(state): Extension<AppState>) -> Json<StatusResponse>;

/// GET /health
pub async fn health() -> &'static str {
    "OK"
}
```

---

### Feature 7: HTTP MCP Transport (SSE)

#### Design Rationale

stdio transport is suitable for local CLI usage but HTTP transport enables:
1. Remote MCP server deployment
2. Multiple clients connecting to one server
3. Browser-based MCP clients
4. Container/cloud deployments

#### Module Structure

```
src/mcp/
  mod.rs                  # Re-exports
  server.rs               # Existing stdio server
  transport/
    mod.rs                # Transport abstraction
    stdio.rs              # Existing stdio transport
    http.rs               # HTTP/SSE transport
```

#### Architecture Diagram

```
Client A (stdio)              Client B (HTTP)
    |                              |
    | stdin/stdout                 | HTTP POST/SSE
    |                              |
    v                              v
+-------------------+      +-------------------+
| StdioTransport    |      | HttpTransport     |
+--------+----------+      +--------+----------+
         |                          |
         +------------+-------------+
                      |
                      v
              +----------------+
              | McpHandler     |
              |----------------|
              | - search       |
              | - storage      |
              +----------------+
```

#### Struct Definitions

```rust
// src/mcp/transport/mod.rs

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

pub use stdio::StdioTransport;
pub use http::HttpTransport;

mod stdio;
mod http;

/// Messages in the MCP protocol
#[derive(Debug, Clone)]
pub enum McpMessage {
    Request { id: String, method: String, params: Value },
    Response { id: String, result: Value },
    Error { id: String, error: McpError },
    Notification { method: String, params: Value },
}

/// Transport layer for MCP communication
#[async_trait]
pub trait Transport: Send + Sync {
    /// Start the transport and begin handling messages
    async fn run(&self) -> Result<()>;

    /// Send a message to the client
    async fn send(&self, message: McpMessage) -> Result<()>;

    /// Get the transport type identifier
    fn transport_type(&self) -> &'static str;
}
```

```rust
// src/mcp/transport/http.rs

use anyhow::Result;
use axum::{Router, routing::{get, post}, Extension};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;

use super::{Transport, McpMessage};
use crate::mcp::McpHandler;

/// HTTP transport configuration
#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub addr: SocketAddr,
    /// Path for SSE events
    pub sse_path: String,
    /// Path for JSON-RPC messages
    pub rpc_path: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:9091".parse().unwrap(),
            sse_path: "/events".to_string(),
            rpc_path: "/rpc".to_string(),
        }
    }
}

/// HTTP/SSE transport for MCP
pub struct HttpTransport {
    config: HttpConfig,
    handler: Arc<McpHandler>,
    event_tx: broadcast::Sender<McpMessage>,
}

impl HttpTransport {
    pub fn new(config: HttpConfig, handler: Arc<McpHandler>) -> Self;

    /// Create the HTTP router
    fn create_router(&self) -> Router;

    /// Handle SSE connection
    /// GET /events - Server-Sent Events stream
    async fn handle_sse(&self) -> impl axum::response::IntoResponse;

    /// Handle JSON-RPC request
    /// POST /rpc - JSON-RPC 2.0 endpoint
    async fn handle_rpc(&self, body: String) -> impl axum::response::IntoResponse;
}

#[async_trait]
impl Transport for HttpTransport {
    async fn run(&self) -> Result<()>;

    async fn send(&self, message: McpMessage) -> Result<()>;

    fn transport_type(&self) -> &'static str {
        "http-sse"
    }
}
```

#### Configuration Additions

```rust
// src/config.rs - Extended ServerConfig

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Transport type: "stdio" (default), "http"
    #[serde(default = "default_transport")]
    pub transport: String,

    /// HTTP transport settings (only used when transport = "http")
    #[serde(default)]
    pub http: HttpServerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpServerConfig {
    /// Host to bind to
    #[serde(default = "default_http_host")]
    pub host: String,
    /// Port to bind to
    #[serde(default = "default_http_port")]
    pub port: u16,
    /// SSE endpoint path
    #[serde(default = "default_sse_path")]
    pub sse_path: String,
    /// RPC endpoint path
    #[serde(default = "default_rpc_path")]
    pub rpc_path: String,
}

fn default_http_host() -> String { "127.0.0.1".to_string() }
fn default_http_port() -> u16 { 9091 }
fn default_sse_path() -> String { "/events".to_string() }
fn default_rpc_path() -> String { "/rpc".to_string() }
```

---

## Cargo.toml Additions

```toml
[package]
name = "coderag"
version = "0.3.0"  # Update version
edition = "2021"

[dependencies]
# Existing dependencies...
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
ignore = "0.4"
walkdir = "2"
fastembed = "4"
lancedb = "0.15"
arrow-array = "53"
arrow-schema = "53"
rmcp = { version = "0.10", features = ["server", "transport-io", "macros"] }
schemars = "1"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4"] }
indicatif = "0.17"
glob = "0.3"
futures = "0.3"

# v0.2 additions
tree-sitter = "0.24"               # AST parsing
tree-sitter-rust = "0.24"          # Rust grammar
tree-sitter-python = "0.23"        # Python grammar
tree-sitter-javascript = "0.23"    # JavaScript grammar
tree-sitter-typescript = "0.23"    # TypeScript grammar
tree-sitter-go = "0.23"            # Go grammar
tree-sitter-java = "0.23"          # Java grammar
notify = "7"                        # File system watching
notify-debouncer-full = "0.4"      # Debounced watching

# v0.3 additions
tantivy = "0.22"                   # BM25 search
async-trait = "0.1"                # Async traits
axum = { version = "0.7", features = ["ws"] }  # Web framework
tower-http = { version = "0.5", features = ["cors", "fs"] }  # HTTP middleware
prometheus = "0.13"                # Metrics
lazy_static = "1.4"                # Static metrics
chrono = { version = "0.4", features = ["serde"] }  # DateTime for registry
directories = "5"                  # Platform-specific directories

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"

[features]
default = ["ast-chunking", "hybrid-search"]
ast-chunking = []
hybrid-search = []
web-ui = ["axum", "tower-http"]
metrics = ["prometheus", "lazy_static"]
```

---

## Implementation Priority

### Phase 1: v0.2 Core (Weeks 1-2)
1. **AST Chunker Infrastructure** - Parser pool, extractor trait
2. **Rust Extractor** - Most complete, serves as template
3. **Python/TypeScript Extractors** - High-demand languages
4. **Watch Mode** - Basic file watching with debouncing

### Phase 2: v0.2 Polish (Weeks 3-4)
5. **Remaining Extractors** - Go, Java, generic fallback
6. **Watch Mode Integration** - Full CLI integration, statistics
7. **Testing** - Unit tests, integration tests

### Phase 3: v0.3 Core (Weeks 5-6)
8. **BM25 Search** - Tantivy integration
9. **Hybrid Search** - RRF fusion
10. **Multi-project Registry** - Global storage, project management

### Phase 4: v0.3 Extras (Weeks 7-8)
11. **Metrics** - Prometheus integration
12. **Web UI** - Basic debug interface
13. **HTTP Transport** - SSE-based MCP transport

---

## Architectural Tradeoffs

### AST Chunking
| Decision | Pros | Cons |
|----------|------|------|
| Per-language extractors | Precise extraction, language-specific features | More code to maintain, need grammar for each language |
| Fallback to line-based | Always works, graceful degradation | Inconsistent chunk quality |
| Include doc comments | Better context for embeddings | Larger chunks, more tokens |

### Watch Mode
| Decision | Pros | Cons |
|----------|------|------|
| Debouncing (500ms) | Avoids redundant work | Slight delay before reindex |
| Batch processing | Efficient embedding generation | Memory usage for large batches |
| In-memory pending queue | Fast, simple | Lost on crash |

### Hybrid Search
| Decision | Pros | Cons |
|----------|------|------|
| RRF fusion (k=60) | Stable, well-studied | May not be optimal for all query types |
| 0.7/0.3 vector/BM25 | Good default for semantic search | May need tuning per codebase |
| Parallel search | Lower latency | More resource usage |

### Multi-project
| Decision | Pros | Cons |
|----------|------|------|
| Global registry | Single source of truth, cross-project search | Additional complexity |
| Optional global storage | Flexibility for users | Two code paths |
| Path-based identification | Simple, deterministic | Breaks if project moves |

### Web UI
| Decision | Pros | Cons |
|----------|------|------|
| Embedded static files | Single binary deployment | Larger binary size |
| Axum framework | Fast, async, good ergonomics | Another dependency |
| WebSocket for watch | Real-time updates | Connection management |

---

## Testing Strategy

### Unit Tests
- Each extractor: Parse known code snippets, verify extracted units
- Debouncer: Verify event coalescing and timing
- RRF fusion: Known rankings produce expected fused order
- Registry: CRUD operations, search functionality

### Integration Tests
- Full indexing pipeline: File -> AST chunks -> Embeddings -> Storage
- Hybrid search: Query -> Vector + BM25 -> Fused results
- Watch mode: File change -> Reindex -> Updated search results
- Web API: HTTP requests -> JSON responses

### Property Tests
- AST chunking never drops code (union of chunks == original)
- RRF fusion is stable (same inputs -> same outputs)
- Debouncing never loses events (all changes eventually processed)

---

## File Summary

New files to create for v0.2:
- `/home/nolood/general/coderag/src/indexer/ast_chunker/mod.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/parser.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/mod.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/rust.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/python.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/typescript.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/go.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/java.rs`
- `/home/nolood/general/coderag/src/indexer/ast_chunker/extractors/generic.rs`
- `/home/nolood/general/coderag/src/watcher/mod.rs`
- `/home/nolood/general/coderag/src/watcher/events.rs`
- `/home/nolood/general/coderag/src/watcher/handler.rs`
- `/home/nolood/general/coderag/src/commands/watch.rs`

New files to create for v0.3:
- `/home/nolood/general/coderag/src/search/bm25/mod.rs`
- `/home/nolood/general/coderag/src/search/bm25/index.rs`
- `/home/nolood/general/coderag/src/search/bm25/schema.rs`
- `/home/nolood/general/coderag/src/search/hybrid.rs`
- `/home/nolood/general/coderag/src/search/fusion.rs`
- `/home/nolood/general/coderag/src/registry/mod.rs`
- `/home/nolood/general/coderag/src/registry/project.rs`
- `/home/nolood/general/coderag/src/registry/global.rs`
- `/home/nolood/general/coderag/src/metrics/mod.rs`
- `/home/nolood/general/coderag/src/metrics/search.rs`
- `/home/nolood/general/coderag/src/metrics/index.rs`
- `/home/nolood/general/coderag/src/web/mod.rs`
- `/home/nolood/general/coderag/src/web/routes/mod.rs`
- `/home/nolood/general/coderag/src/web/routes/search.rs`
- `/home/nolood/general/coderag/src/web/routes/status.rs`
- `/home/nolood/general/coderag/src/web/routes/metrics.rs`
- `/home/nolood/general/coderag/src/mcp/transport/mod.rs`
- `/home/nolood/general/coderag/src/mcp/transport/stdio.rs`
- `/home/nolood/general/coderag/src/mcp/transport/http.rs`

Files to modify:
- `/home/nolood/general/coderag/src/lib.rs` - Add new module exports
- `/home/nolood/general/coderag/src/indexer/mod.rs` - Add ast_chunker module
- `/home/nolood/general/coderag/src/indexer/chunker.rs` - Extend Chunk struct
- `/home/nolood/general/coderag/src/search/mod.rs` - Add Search trait, new modules
- `/home/nolood/general/coderag/src/search/vector.rs` - Implement Search trait
- `/home/nolood/general/coderag/src/storage/lancedb.rs` - Extended IndexedChunk schema
- `/home/nolood/general/coderag/src/config.rs` - New configuration sections
- `/home/nolood/general/coderag/src/commands/mod.rs` - Add watch command
- `/home/nolood/general/coderag/src/commands/index.rs` - Use AstChunker
- `/home/nolood/general/coderag/src/mcp/mod.rs` - Add transport module
- `/home/nolood/general/coderag/Cargo.toml` - New dependencies
