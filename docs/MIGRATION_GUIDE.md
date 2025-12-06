# Migration Guide

This guide helps existing CodeRAG users upgrade to the latest version with new features and improvements.

## Version Compatibility

| From Version | To Version | Breaking Changes | Migration Required |
|--------------|------------|------------------|-------------------|
| 0.0.x | 0.1.0 | Yes | Configuration update |
| 0.1.0 | 0.2.0 | No | Optional enhancements |

## Major Changes in Latest Version

### New Features
- ✅ OpenAI embedding provider support
- ✅ Parallel indexing (3-5x speedup)
- ✅ File header injection in search results
- ✅ Symbol search MCP tools (find_symbol, list_symbols, find_references)
- ✅ Smart batch detection for watch mode
- ✅ C/C++ language support
- ✅ Comprehensive test suite (150+ tests)
- ✅ Performance benchmarks

### Improvements
- 🚀 3-5x faster indexing with parallel processing
- 📊 Better search quality with file context
- 🔍 More powerful symbol navigation
- 🛠️ Smarter file watching with batch detection
- 📝 Extended language support

## Migration Steps

### Step 1: Backup Current Configuration

```bash
# Backup your current configuration
cp .coderag/config.toml .coderag/config.toml.backup

# Backup your index (optional, will be rebuilt)
cp -r .coderag/index.lance .coderag/index.lance.backup
```

### Step 2: Update CodeRAG

```bash
# If installed via cargo
cargo install coderag --force

# Or pull latest and build
git pull origin main
cargo build --release
```

### Step 3: Update Configuration

#### Enable OpenAI Embeddings (Optional)

If you want to use OpenAI embeddings instead of local FastEmbed:

```toml
# Before (old config)
[embeddings]
model = "nomic-embed-text-v1.5"
batch_size = 32

# After (new config)
[embeddings]
provider = "openai"  # or "fastembed" to keep using local

[embeddings.providers.openai]
api_key = "${OPENAI_API_KEY}"  # Set environment variable
model = "text-embedding-3-small"
batch_size = 100

[embeddings.providers.fastembed]
model = "nomic-embed-text-v1.5"
batch_size = 32
```

#### Enable Parallel Indexing

Add these settings for faster indexing:

```toml
[indexer]
# Existing settings...
parallel_threads = null  # Auto-detect CPU cores
file_batch_size = 100
max_concurrent_files = 50
```

#### Configure Smart Batch Detection

For better watch mode performance:

```toml
[watcher]
debounce_ms = 500

[watcher.mass_change]
threshold_files = 50
threshold_rate = 20.0
collection_delay_ms = 3000
```

#### Enable File Header Injection

For better search context:

```toml
[search]
# Existing settings...
include_file_header = true
file_header_lines = 50
```

#### Add C/C++ Support

Update file extensions:

```toml
[indexer]
extensions = [
    "rs", "py", "ts", "tsx", "js", "jsx",
    "go", "java",
    "c", "cpp", "cc", "cxx", "h", "hpp"  # New C/C++ extensions
]
```

### Step 4: Re-index Your Codebase

After updating configuration, re-index to use new features:

```bash
# Force complete re-indexing
coderag index --force

# Or if you want to see progress
coderag index --verbose
```

### Step 5: Update MCP Configuration (If Using)

If using with Claude or other LLM via MCP:

```json
// claude_desktop_config.json
{
  "mcpServers": {
    "coderag": {
      "command": "coderag",
      "args": ["serve"],
      "env": {
        "OPENAI_API_KEY": "sk-..."  // If using OpenAI embeddings
      }
    }
  }
}
```

### Step 6: Test New Features

#### Test Parallel Indexing
```bash
# Check indexing speed
time coderag index --force
# Should see 3-5x improvement
```

#### Test Symbol Search
```bash
# Try new symbol search
coderag search --symbol "MyClass" --kind class
```

#### Test File Header Injection
```bash
# Search and check for file headers
coderag search "database connection"
# Results should include first 50 lines of files
```

## Configuration Migration Examples

### Example 1: Minimal Migration (Keep Existing Behavior)

If you want minimal changes and keep existing behavior:

```toml
# Just add version and keep defaults
version = "0.2.0"

[indexer]
# Your existing settings...
# Add only if you want parallel indexing
parallel_threads = 4  # Conservative parallel setting

[embeddings]
provider = "fastembed"  # Keep using local embeddings

[embeddings.providers.fastembed]
model = "nomic-embed-text-v1.5"  # Your existing model
```

### Example 2: Performance-Focused Migration

For maximum performance improvements:

```toml
version = "0.2.0"

[indexer]
extensions = ["rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp"]
parallel_threads = null  # Auto-detect all cores
file_batch_size = 200
max_concurrent_files = 100

[embeddings]
provider = "fastembed"

[embeddings.providers.fastembed]
model = "all-MiniLM-L6-v2"  # Fastest model
batch_size = 64

[search]
mode = "vector"  # Fastest search
include_file_header = false  # Skip for speed
```

### Example 3: Quality-Focused Migration

For best search quality:

```toml
version = "0.2.0"

[indexer]
extensions = ["rs", "py", "ts", "tsx", "js", "jsx", "go", "java", "c", "cpp"]
chunk_size = 768
chunker_strategy = "ast"
parallel_threads = 8

[embeddings]
provider = "openai"

[embeddings.providers.openai]
api_key = "${OPENAI_API_KEY}"
model = "text-embedding-3-large"
batch_size = 50

[search]
mode = "hybrid"
vector_weight = 0.7
bm25_weight = 0.3
include_file_header = true
file_header_lines = 100  # More context
```

## Breaking Changes

### Configuration Format Changes

#### Old Format (v0.0.x)
```toml
[embeddings]
model = "nomic-embed-text-v1.5"
batch_size = 32
```

#### New Format (v0.1.0+)
```toml
[embeddings]
provider = "fastembed"

[embeddings.providers.fastembed]
model = "nomic-embed-text-v1.5"
batch_size = 32
```

### API Changes (For Library Users)

#### Search API
```rust
// Old API
let results = index.search(query, limit).await?;

// New API with options
let results = index.search(SearchOptions {
    query,
    limit,
    include_file_header: true,
    mode: SearchMode::Hybrid,
}).await?;
```

#### Symbol Search API
```rust
// New API (didn't exist before)
let symbols = index.find_symbol(SymbolQuery {
    name: "MyClass",
    kind: Some(SymbolKind::Class),
    mode: MatchMode::Exact,
}).await?;
```

## Rollback Procedure

If you encounter issues and need to rollback:

### Step 1: Restore Configuration
```bash
cp .coderag/config.toml.backup .coderag/config.toml
```

### Step 2: Restore Index (Optional)
```bash
rm -rf .coderag/index.lance
cp -r .coderag/index.lance.backup .coderag/index.lance
```

### Step 3: Downgrade CodeRAG
```bash
# Install specific version
cargo install coderag --version 0.0.1 --force
```

## Troubleshooting

### Issue: Index Compatibility Error

**Error**: "Index version mismatch"

**Solution**: Re-index with new version
```bash
rm -rf .coderag/index.lance
coderag index
```

### Issue: Configuration Parse Error

**Error**: "Failed to parse configuration"

**Solution**: Update configuration format
```bash
# Check configuration
coderag validate-config

# Or use migration tool
coderag migrate-config
```

### Issue: Embedding Model Not Found

**Error**: "Model 'xyz' not found"

**Solution**: Update model name or install
```toml
[embeddings.providers.fastembed]
model = "nomic-embed-text-v1.5"  # Use supported model
```

### Issue: OpenAI API Key Not Set

**Error**: "OpenAI API key not found"

**Solution**: Set environment variable
```bash
export OPENAI_API_KEY="sk-..."
coderag index
```

### Issue: Slow Indexing After Update

**Solution**: Enable parallel indexing
```toml
[indexer]
parallel_threads = null  # Auto-detect
```

## Feature Comparison

| Feature | Old Version | New Version | Benefit |
|---------|------------|-------------|---------|
| Indexing Speed | 100 files/sec | 300+ files/sec | 3x faster |
| Embedding Providers | FastEmbed only | FastEmbed + OpenAI | More choice |
| Symbol Search | Basic | Advanced with MCP tools | Better navigation |
| Language Support | 6 languages | 8 languages (+ C/C++) | Wider coverage |
| File Context | None | 50-line headers | Better understanding |
| Batch Detection | Basic debounce | Smart detection | Handles git better |
| Test Coverage | ~50 tests | 150+ tests | More reliable |

## Migration Checklist

- [ ] Backup current configuration
- [ ] Backup current index (optional)
- [ ] Update CodeRAG to latest version
- [ ] Update configuration file format
- [ ] Enable desired new features
- [ ] Re-index codebase
- [ ] Test symbol search tools
- [ ] Verify search quality
- [ ] Update MCP configuration if needed
- [ ] Test with your LLM integration

## Getting Help

### Resources
- GitHub Issues: https://github.com/nolood/coderag/issues
- Documentation: /docs/
- Discord: [Community Server]

### Common Migration Paths

#### From Local-Only to Cloud Embeddings
1. Set up OpenAI API key
2. Update embeddings configuration
3. Re-index with new provider
4. Compare search quality

#### From Sequential to Parallel Indexing
1. Add parallel configuration
2. Test with different thread counts
3. Monitor CPU and memory usage
4. Find optimal settings

#### From Basic to Enhanced Search
1. Enable file header injection
2. Configure hybrid search weights
3. Test symbol search tools
4. Adjust based on results

## Version History

### v0.2.0 (Latest)
- Added OpenAI embeddings
- Implemented parallel indexing
- Added symbol search tools
- Added C/C++ support
- Improved test coverage

### v0.1.0
- Initial MCP server implementation
- Basic FastEmbed support
- Core search functionality
- Watch mode

### v0.0.x
- Beta releases
- Experimental features