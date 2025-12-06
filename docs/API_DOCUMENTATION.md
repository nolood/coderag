# API Documentation

CodeRAG provides a comprehensive Rust API for programmatic access to all features. This document covers the main API modules and usage examples.

## Generated Documentation

The full API documentation can be generated and viewed locally:

```bash
# Generate documentation
cargo doc --no-deps --open

# Generate with private items
cargo doc --no-deps --document-private-items --open
```

Documentation is generated at: `target/doc/coderag/index.html`

## Core Modules

### Storage Module (`coderag::storage`)

Handles vector storage and retrieval using LanceDB.

```rust
use coderag::storage::{Storage, StorageConfig};

// Initialize storage
let config = StorageConfig::default();
let storage = Storage::new(config).await?;

// Add chunks to index
let chunks = vec![
    Chunk {
        content: "fn process_data() { ... }".to_string(),
        file_path: "src/main.rs".to_string(),
        start_line: 10,
        end_line: 20,
        embeddings: vec![0.1, 0.2, ...],
    }
];
storage.add_chunks(chunks).await?;

// Search vectors
let results = storage.vector_search("query", 10).await?;
```

### Indexer Module (`coderag::indexer`)

Provides code chunking and AST analysis.

```rust
use coderag::indexer::{Indexer, IndexerConfig, ChunkingStrategy};

// Configure indexer
let config = IndexerConfig {
    chunk_size: 512,
    strategy: ChunkingStrategy::Ast,
    parallel_threads: Some(8),
    ..Default::default()
};

let indexer = Indexer::new(config);

// Index a directory
let stats = indexer.index_directory("/path/to/project").await?;
println!("Indexed {} files, created {} chunks", stats.files, stats.chunks);
```

### Embeddings Module (`coderag::embeddings`)

Generates embeddings using various providers.

```rust
use coderag::embeddings::{EmbeddingProvider, FastEmbedProvider, OpenAIProvider};

// FastEmbed (local)
let provider = FastEmbedProvider::new(
    "nomic-embed-text-v1.5",
    32, // batch_size
)?;

// OpenAI
let provider = OpenAIProvider::new(
    "sk-...",
    "text-embedding-3-small",
    100, // batch_size
)?;

// Generate embeddings
let texts = vec!["code snippet 1", "code snippet 2"];
let embeddings = provider.embed_batch(texts).await?;
```

### Search Module (`coderag::search`)

Implements various search strategies.

```rust
use coderag::search::{SearchEngine, SearchMode, SearchOptions};

let engine = SearchEngine::new(storage, bm25_index)?;

// Configure search
let options = SearchOptions {
    mode: SearchMode::Hybrid,
    vector_weight: 0.7,
    bm25_weight: 0.3,
    limit: 10,
    include_file_header: true,
};

// Perform search
let results = engine.search("authentication logic", options).await?;

// Symbol search
let symbols = engine.find_symbol(SymbolQuery {
    name: "MyClass",
    kind: Some(SymbolKind::Class),
    mode: MatchMode::Exact,
    limit: 10,
}).await?;
```

### MCP Module (`coderag::mcp`)

Model Context Protocol server implementation.

```rust
use coderag::mcp::{McpServer, Transport};

// Create MCP server
let server = McpServer::new(search_engine);

// Stdio transport
server.run_stdio().await?;

// HTTP transport
server.run_http("127.0.0.1:3000").await?;

// Handle tool calls
let response = server.handle_tool_call("search", json!({
    "query": "database connection"
})).await?;
```

### Watcher Module (`coderag::watcher`)

File system monitoring and auto-indexing.

```rust
use coderag::watcher::{FileWatcher, WatcherConfig};

let config = WatcherConfig {
    debounce_ms: 500,
    mass_change_threshold: 50,
    ..Default::default()
};

let watcher = FileWatcher::new(config, indexer);

// Start watching
watcher.watch("/path/to/project").await?;

// Handle events
watcher.on_change(|event| {
    println!("File changed: {:?}", event);
}).await;
```

## Key Types and Traits

### Chunk
```rust
pub struct Chunk {
    pub id: String,
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Language,
    pub embeddings: Vec<f32>,
    pub metadata: ChunkMetadata,
}
```

### Symbol
```rust
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
}

pub enum SymbolKind {
    Function,
    Class,
    Method,
    Variable,
    Constant,
    Type,
    Interface,
    Enum,
    Module,
}
```

### SearchResult
```rust
pub struct SearchResult {
    pub chunk: Chunk,
    pub score: f32,
    pub file_header: Option<String>,
    pub highlights: Vec<TextRange>,
}
```

### EmbeddingProvider Trait
```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}
```

## Usage Examples

### Complete Indexing Pipeline

```rust
use coderag::{Indexer, Storage, EmbeddingProvider, FastEmbedProvider};

async fn index_project(path: &str) -> Result<()> {
    // Initialize components
    let storage = Storage::new(StorageConfig::default()).await?;
    let embedder = FastEmbedProvider::new("nomic-embed-text-v1.5", 32)?;
    let indexer = Indexer::new(IndexerConfig::default());

    // Scan files
    let files = indexer.scan_directory(path)?;
    println!("Found {} files", files.len());

    // Process files in parallel
    let chunks = indexer.process_files_parallel(files, 8).await?;
    println!("Created {} chunks", chunks.len());

    // Generate embeddings
    let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
    let embeddings = embedder.embed_batch(texts).await?;

    // Store in database
    let mut chunks_with_embeddings = chunks;
    for (chunk, embedding) in chunks_with_embeddings.iter_mut().zip(embeddings) {
        chunk.embeddings = embedding;
    }

    storage.add_chunks(chunks_with_embeddings).await?;
    println!("Indexing complete!");

    Ok(())
}
```

### Custom Search Implementation

```rust
use coderag::search::{SearchEngine, CustomScorer};

struct MyCustomScorer;

impl CustomScorer for MyCustomScorer {
    fn score(&self, query: &str, chunk: &Chunk) -> f32 {
        // Custom scoring logic
        let keyword_match = chunk.content.contains(query) as i32 as f32;
        let length_penalty = 1.0 / (chunk.content.len() as f32).sqrt();
        keyword_match * 0.5 + length_penalty * 0.5
    }
}

async fn custom_search() -> Result<()> {
    let engine = SearchEngine::new(storage, bm25)?;
    engine.set_custom_scorer(Box::new(MyCustomScorer));

    let results = engine.search("query", SearchOptions::default()).await?;
    Ok(())
}
```

### Symbol Extraction

```rust
use coderag::indexer::symbol_extraction::{extract_symbols, Language};

fn extract_rust_symbols(code: &str) -> Vec<Symbol> {
    let symbols = extract_symbols(code, Language::Rust)?;

    for symbol in &symbols {
        println!(
            "{:?} '{}' at line {}",
            symbol.kind, symbol.name, symbol.line
        );
    }

    symbols
}
```

### Streaming Results

```rust
use futures::StreamExt;
use coderag::search::SearchStream;

async fn stream_search_results() -> Result<()> {
    let mut stream = engine.search_stream("query", 100);

    while let Some(result) = stream.next().await {
        match result {
            Ok(chunk) => println!("Found: {}", chunk.file_path),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    Ok(())
}
```

## Error Handling

CodeRAG uses a custom error type with detailed context:

```rust
use coderag::{Result, Error, ErrorKind};

fn handle_errors() -> Result<()> {
    match do_something() {
        Ok(value) => Ok(value),
        Err(Error::Storage(e)) => {
            eprintln!("Storage error: {}", e);
            Err(e.into())
        },
        Err(Error::Indexing(e)) => {
            eprintln!("Indexing error: {}", e);
            Err(e.into())
        },
        Err(e) => Err(e),
    }
}
```

## Configuration

### Loading Configuration

```rust
use coderag::config::{Config, load_config};

// Load from file
let config = load_config(".coderag/config.toml")?;

// Or create programmatically
let config = Config {
    indexer: IndexerConfig {
        chunk_size: 512,
        parallel_threads: Some(8),
        ..Default::default()
    },
    embeddings: EmbeddingsConfig {
        provider: "openai".to_string(),
        ..Default::default()
    },
    ..Default::default()
};
```

## Testing Support

CodeRAG provides testing utilities:

```rust
#[cfg(test)]
mod tests {
    use coderag::test_utils::{setup_test_index, create_test_chunks};

    #[tokio::test]
    async fn test_search() {
        let index = setup_test_index().await;
        let chunks = create_test_chunks(10);

        index.add_chunks(chunks).await.unwrap();

        let results = index.search("test", 5).await.unwrap();
        assert_eq!(results.len(), 5);
    }
}
```

## Performance Considerations

### Batching Operations

```rust
// Batch embedding generation
let batch_size = 100;
for chunk in texts.chunks(batch_size) {
    let embeddings = provider.embed_batch(chunk).await?;
    // Process embeddings
}

// Batch database insertions
storage.add_chunks_batched(chunks, 1000).await?;
```

### Parallel Processing

```rust
use rayon::prelude::*;

// Parallel file processing
let chunks: Vec<_> = files
    .par_iter()
    .flat_map(|file| chunk_file(file))
    .collect();

// Parallel embedding generation
let embeddings: Vec<_> = chunks
    .par_chunks(100)
    .flat_map(|batch| embed_batch(batch))
    .collect();
```

## Extending CodeRAG

### Custom Language Support

```rust
use coderag::indexer::{LanguageChunker, Language};

struct MyLanguageChunker;

impl LanguageChunker for MyLanguageChunker {
    fn chunk(&self, content: &str) -> Vec<Chunk> {
        // Custom chunking logic
        vec![]
    }

    fn extract_symbols(&self, content: &str) -> Vec<Symbol> {
        // Custom symbol extraction
        vec![]
    }
}

// Register custom chunker
indexer.register_chunker(Language::Custom("mylang"), Box::new(MyLanguageChunker));
```

### Custom Embedding Provider

```rust
#[async_trait]
impl EmbeddingProvider for MyCustomProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Custom embedding logic
        Ok(vec![0.0; 768])
    }

    async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        // Batch embedding logic
        Ok(texts.iter().map(|_| vec![0.0; 768]).collect())
    }

    fn dimension(&self) -> usize {
        768
    }
}
```

## API Stability

| Module | Stability | Since |
|--------|-----------|-------|
| storage | Stable | 0.1.0 |
| indexer | Stable | 0.1.0 |
| search | Stable | 0.1.0 |
| embeddings | Beta | 0.2.0 |
| mcp | Beta | 0.2.0 |
| config | Stable | 0.1.0 |

## See Also

- [Rust API Docs](https://docs.rs/coderag) - Published crate documentation
- [GitHub Repository](https://github.com/nolood/coderag) - Source code
- [Examples](https://github.com/nolood/coderag/tree/main/examples) - Usage examples