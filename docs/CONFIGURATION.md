# Configuration Guide

CodeRAG provides extensive configuration options to customize indexing, search, and embedding generation for your specific needs.

## Configuration File

CodeRAG uses a TOML configuration file located at `.coderag/config.toml` in your project root. This file is created automatically when you run `coderag init`.

## Complete Configuration Reference

```toml
# Full configuration with all options and defaults

[indexer]
# File extensions to index
extensions = ["rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp", "cc", "cxx", "h", "hpp"]

# Patterns to ignore during indexing
ignore_patterns = ["node_modules", "target", ".git", "dist", "build", "vendor", ".venv"]

# Token size for code chunks
chunk_size = 512

# Chunking strategy: "ast" (AST-based) or "line" (line-based)
chunker_strategy = "ast"

# Minimum tokens in a chunk
min_chunk_tokens = 50

# Maximum tokens in a chunk
max_chunk_tokens = 1500

# Number of parallel indexing threads
# null = auto-detect based on CPU cores
parallel_threads = 8  # or null for auto

# Number of files to process in a batch
file_batch_size = 100

# Maximum concurrent file processing
max_concurrent_files = 50

[embeddings]
# Embedding provider: "fastembed" or "openai"
provider = "fastembed"

[embeddings.providers.fastembed]
# FastEmbed model selection
model = "nomic-embed-text-v1.5"
# Other options:
# - "all-MiniLM-L6-v2" (384 dims, fastest)
# - "bge-small-en-v1.5" (384 dims)
# - "bge-base-en-v1.5" (768 dims)
# - "bge-large-en-v1.5" (1024 dims, best quality)

# Batch size for embedding generation
batch_size = 32

[embeddings.providers.openai]
# OpenAI API key (supports environment variable)
api_key = "${OPENAI_API_KEY}"

# OpenAI embedding model
model = "text-embedding-3-small"
# Other options:
# - "text-embedding-3-large" (3072 dims, best quality)
# - "text-embedding-ada-002" (1536 dims, legacy)

# Batch size for API requests
batch_size = 100

[storage]
# Database path relative to .coderag/
db_path = "index.lance"

[server]
# Transport type: "stdio" or "http"
transport = "stdio"

[server.http]
# HTTP server host
host = "127.0.0.1"

# HTTP server port
port = 3000

# Server-Sent Events path
sse_path = "/sse"

# POST endpoint path
post_path = "/message"

[search]
# Search mode: "vector", "bm25", or "hybrid"
mode = "hybrid"

# Weight for vector search (0.0 - 1.0)
vector_weight = 0.7

# Weight for BM25 search (0.0 - 1.0)
bm25_weight = 0.3

# RRF constant for result fusion
rrf_k = 60.0

# Default number of search results
default_limit = 10

# Include file header in search results
include_file_header = true

# Number of lines to include in file header
file_header_lines = 50

[watcher]
# Debounce delay in milliseconds
debounce_ms = 500

[watcher.mass_change]
# Number of files threshold for mass change detection
threshold_files = 50

# Rate of change threshold (files per second)
threshold_rate = 20.0

# Collection delay for batch processing (ms)
collection_delay_ms = 3000
```

## Configuration Sections Explained

### Indexer Configuration

Controls how CodeRAG processes and chunks your code files.

#### Parallel Processing
```toml
[indexer]
parallel_threads = 8  # Set to null for auto-detection
file_batch_size = 100
max_concurrent_files = 50
```

- **parallel_threads**: Number of CPU threads for parallel indexing
  - Set to `null` to auto-detect based on CPU cores
  - Range: 1-16 threads
  - Higher values speed up indexing but use more CPU

- **file_batch_size**: Files processed per batch
  - Affects memory usage and progress reporting
  - Lower values = more frequent progress updates

- **max_concurrent_files**: Maximum files processed simultaneously
  - Prevents memory exhaustion on large codebases
  - Tune based on available RAM

#### Chunking Strategy
```toml
[indexer]
chunker_strategy = "ast"  # or "line"
chunk_size = 512
min_chunk_tokens = 50
max_chunk_tokens = 1500
```

- **ast**: Uses Tree-sitter for semantic code splitting
  - Preserves function/class boundaries
  - Better search accuracy
  - Supported for: Rust, Python, JS/TS, Go, Java, C/C++

- **line**: Simple line-based splitting
  - Faster processing
  - Works with any text file
  - Less semantic awareness

### Embedding Providers

#### FastEmbed (Local)
```toml
[embeddings]
provider = "fastembed"

[embeddings.providers.fastembed]
model = "nomic-embed-text-v1.5"
batch_size = 32
```

**Advantages:**
- No API costs
- Data stays local
- Fast processing
- Auto-downloads models

**Models:**
| Model | Dimensions | Speed | Quality |
|-------|------------|-------|---------|
| all-MiniLM-L6-v2 | 384 | Fastest | Good |
| nomic-embed-text-v1.5 | 768 | Fast | Excellent |
| bge-small-en-v1.5 | 384 | Fast | Good |
| bge-base-en-v1.5 | 768 | Medium | Very Good |
| bge-large-en-v1.5 | 1024 | Slow | Best |

#### OpenAI (Cloud)
```toml
[embeddings]
provider = "openai"

[embeddings.providers.openai]
api_key = "${OPENAI_API_KEY}"  # Uses environment variable
model = "text-embedding-3-small"
batch_size = 100
```

**Advantages:**
- State-of-the-art quality
- No local model storage
- Consistent performance

**Models:**
| Model | Dimensions | Cost | Quality |
|-------|------------|------|---------|
| text-embedding-3-small | 1536 | Low | Very Good |
| text-embedding-3-large | 3072 | Medium | Excellent |
| text-embedding-ada-002 | 1536 | Low | Good (Legacy) |

### Search Configuration

```toml
[search]
mode = "hybrid"
vector_weight = 0.7
bm25_weight = 0.3
rrf_k = 60.0
```

#### Search Modes

1. **vector**: Pure semantic search
   - Best for: Conceptual queries, finding similar code
   - Example: "authentication logic"

2. **bm25**: Traditional keyword search
   - Best for: Exact term matching, specific identifiers
   - Example: "getUserById"

3. **hybrid**: Combines both approaches (recommended)
   - Best for: General use, balanced accuracy
   - Weights can be tuned for your use case

#### Weight Tuning

- **More vector weight (0.8-0.9)**: Better for understanding intent
- **More BM25 weight (0.5-0.7)**: Better for specific terms
- **Balanced (0.7/0.3)**: Good default for most codebases

### Watcher Configuration

```toml
[watcher]
debounce_ms = 500

[watcher.mass_change]
threshold_files = 50
threshold_rate = 20.0
collection_delay_ms = 3000
```

#### Smart Batch Detection

Automatically detects and handles mass file changes from:
- Git operations (checkout, pull, merge)
- Package manager operations (npm install, cargo build)
- Build system outputs

**Parameters:**
- **threshold_files**: Minimum files to trigger batch mode
- **threshold_rate**: Files/second to detect rapid changes
- **collection_delay_ms**: Wait time to collect all changes

## Environment Variables

CodeRAG supports environment variables in configuration:

```toml
[embeddings.providers.openai]
api_key = "${OPENAI_API_KEY}"
```

Set environment variables:
```bash
export OPENAI_API_KEY="sk-..."
coderag index
```

## Configuration Profiles

### Performance Profile
```toml
# Maximize indexing speed
[indexer]
parallel_threads = null  # Auto-detect all cores
file_batch_size = 200
max_concurrent_files = 100

[embeddings.providers.fastembed]
model = "all-MiniLM-L6-v2"  # Fastest model
batch_size = 64
```

### Quality Profile
```toml
# Maximize search quality
[indexer]
chunker_strategy = "ast"
chunk_size = 768
min_chunk_tokens = 100

[embeddings]
provider = "openai"

[embeddings.providers.openai]
model = "text-embedding-3-large"
```

### Balanced Profile (Default)
```toml
# Good balance of speed and quality
[indexer]
parallel_threads = 8
chunk_size = 512

[embeddings]
provider = "fastembed"

[embeddings.providers.fastembed]
model = "nomic-embed-text-v1.5"
```

## Migration from Previous Versions

### Updating Embedding Provider

To switch from FastEmbed to OpenAI:

1. Update config:
```toml
[embeddings]
provider = "openai"  # was "fastembed"

[embeddings.providers.openai]
api_key = "${OPENAI_API_KEY}"
model = "text-embedding-3-small"
```

2. Re-index your codebase:
```bash
coderag index --force
```

### Enabling Parallel Indexing

Add to existing config:
```toml
[indexer]
parallel_threads = null  # Auto-detect
file_batch_size = 100
max_concurrent_files = 50
```

## Troubleshooting

### Slow Indexing
- Increase `parallel_threads`
- Use faster embedding model
- Check disk I/O performance

### High Memory Usage
- Reduce `max_concurrent_files`
- Decrease `file_batch_size`
- Use smaller embedding model

### Poor Search Quality
- Switch to AST chunking
- Increase `chunk_size`
- Try hybrid search mode
- Adjust search weights

## Best Practices

1. **Start with defaults**: The default configuration works well for most projects

2. **Tune incrementally**: Change one parameter at a time and measure impact

3. **Monitor metrics**: Use `coderag stats` to track performance

4. **Use appropriate models**:
   - Small codebases: Fast models (all-MiniLM-L6-v2)
   - Large codebases: Balanced models (nomic-embed-text-v1.5)
   - Critical accuracy: Best models (OpenAI text-embedding-3-large)

5. **Configure ignore patterns**: Exclude build artifacts and dependencies

6. **Set up watch mode**: Use appropriate debounce and mass change thresholds