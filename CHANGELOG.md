# Changelog

All notable changes to CodeRAG will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2024-12-06

### Added
- **OpenAI Embedding Provider Support** - Use OpenAI's state-of-the-art embedding models (text-embedding-3-small, text-embedding-3-large)
- **Parallel Indexing** - 3-5x faster indexing with Rayon-based parallel processing
- **File Header Injection** - First 50 lines of files now included in search results for better context
- **Symbol Search MCP Tools** - Three new powerful MCP tools:
  - `find_symbol` - Search for symbols by name with exact, prefix, contains, or fuzzy matching
  - `list_symbols` - List all symbols in a specific file with hierarchical structure
  - `find_references` - Find all references to a symbol across the codebase
- **Smart Batch Detection** - Intelligent detection of mass file changes (git operations, npm install, etc.)
- **C/C++ Language Support** - Full AST-based chunking and symbol extraction for C and C++
- **Comprehensive Test Suite** - 150+ tests including unit, integration, and benchmark tests
- **Search Quality Benchmarks** - Precision, recall, and F1 score metrics for search quality

### Changed
- **Embedding Provider Architecture** - Now uses trait-based abstraction for multiple providers
- **Storage Schema** - Extended with symbol metadata and file headers
- **Configuration Format** - New nested structure for provider-specific settings
- **Watch Mode** - Now handles git operations and mass changes gracefully
- **Error Handling** - Improved error messages and recovery strategies

### Performance Improvements
- **Indexing Speed** - 300+ files/sec with parallel processing (vs 100 files/sec sequential)
- **Symbol Search** - <10ms for exact lookups using optimized indexes
- **Memory Efficiency** - Better batching prevents OOM on large codebases
- **Search Latency** - <50ms typical latency for semantic search

### Fixed
- All clippy warnings resolved
- Test infrastructure stabilized
- Edge cases in C/C++ parsing
- Memory leaks in long-running watch mode
- Race conditions in parallel indexing

## [0.1.0] - 2024-11-15

### Added
- **MCP Server Implementation** - Full Model Context Protocol support
- **Hybrid Search** - Combines vector and BM25 search with RRF
- **Watch Mode** - Auto-reindex on file changes
- **Web Interface** - Browser-based search and exploration
- **Multi-Project Support** - Global registry for managing multiple codebases
- **Prometheus Metrics** - Export performance metrics

### Changed
- Improved chunking algorithm
- Better error handling
- Updated dependencies

### Fixed
- Memory leaks in vector search
- Incorrect line numbers in chunks
- Watch mode missing file deletions

## [0.0.5] - 2024-10-20

### Added
- Basic vector search with FastEmbed
- AST-based code chunking
- Support for Rust, Python, TypeScript, JavaScript, Go, Java
- CLI interface
- Basic configuration support

### Changed
- Initial public release

## [0.0.1] - 2024-09-15

### Added
- Project initialization
- Basic project structure
- Core abstractions

---

## Upgrade Guide

### From 0.1.0 to Latest

1. **Update Configuration Format**
   ```toml
   # Old format
   [embeddings]
   model = "nomic-embed-text-v1.5"

   # New format
   [embeddings]
   provider = "fastembed"

   [embeddings.providers.fastembed]
   model = "nomic-embed-text-v1.5"
   ```

2. **Enable New Features**
   ```toml
   [indexer]
   parallel_threads = null  # Auto-detect

   [search]
   include_file_header = true
   ```

3. **Re-index Your Codebase**
   ```bash
   coderag index --force
   ```

### From 0.0.x to Latest

See [Migration Guide](docs/MIGRATION_GUIDE.md) for detailed instructions.

## Deprecated Features

### Deprecated in Latest Version
- Single-provider embedding configuration (use provider-specific config)
- Sequential-only indexing (parallel is now default)
- Old symbol search API (use new MCP tools)

### Removal Timeline
- v0.3.0 - Remove old configuration format support
- v0.4.0 - Remove sequential-only indexing option
- v0.5.0 - Remove deprecated APIs

## Known Issues

### Current Issues
- Large C++ template files may cause slow parsing
- OpenAI rate limiting not fully handled (workaround: reduce batch size)
- Symbol extraction for macro-heavy C code needs improvement

### Under Investigation
- Memory usage spikes with very large repositories (>50k files)
- Occasional false positives in smart batch detection
- Cross-file symbol resolution accuracy

## Contributors

- [@nolood](https://github.com/nolood) - Creator and maintainer
- Community contributors - Bug reports and feature requests

## License

MIT License - See [LICENSE](LICENSE) file for details

## Links

- [GitHub Repository](https://github.com/nolood/coderag)
- [Documentation](docs/)
- [Issue Tracker](https://github.com/nolood/coderag/issues)
- [Discussions](https://github.com/nolood/coderag/discussions)