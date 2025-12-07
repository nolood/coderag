# CodeRAG 🔍

> Semantic code search and navigation powered by AI embeddings and AST analysis

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![MIT License](https://img.shields.io/badge/License-MIT-green.svg)](https://choosealicense.com/licenses/mit/)
[![Tests](https://img.shields.io/badge/tests-150%2B-brightgreen)](docs/TESTING.md)
[![Performance](https://img.shields.io/badge/indexing-300%2B%20files%2Fsec-blue)](docs/PERFORMANCE.md)

CodeRAG is a high-performance semantic code search tool that combines vector embeddings with traditional search techniques. It provides intelligent code navigation through an MCP server, enabling LLMs like Claude to understand and explore codebases effectively.

## ✨ Features

### 🚀 Blazing Fast Performance
- **Parallel indexing**: 300+ files/sec with 3-5x speedup
- **Sub-50ms search latency** for most queries
- **Memory efficient** with smart batching

### 🤖 AI-Powered Search
- **Multiple embedding providers**: FastEmbed (local) and OpenAI
- **Hybrid search**: Combines semantic and keyword matching
- **File context injection**: First 50 lines included in results

### 🔧 Advanced Symbol Navigation
- **find_symbol**: Search functions, classes, variables by name
- **list_symbols**: Browse file structure and symbols
- **find_references**: Track symbol usage across codebase

### 🌍 Extensive Language Support
- Rust, Python, TypeScript, JavaScript
- Go, Java, C, C++
- AST-based intelligent code chunking

### 🔄 Smart File Monitoring
- Auto-reindex on changes
- Intelligent batch detection for git operations
- Configurable debouncing

## 📦 Installation

### Prerequisites

- **Rust** (1.70+)
- **Protocol Buffers compiler** (`protoc`)

```bash
# macOS
brew install protobuf

# Ubuntu/Debian
sudo apt install protobuf-compiler

# Arch Linux
sudo pacman -S protobuf
```

### From Source
```bash
git clone https://github.com/nolood/coderag.git
cd coderag
cargo build --release
cargo install --path .
```

### Using Cargo
```bash
cargo install coderag
```

## 🚀 Quick Start

### 1. Initialize Project
```bash
cd your-project
coderag init
```

### 2. Index Your Codebase
```bash
# Basic indexing
coderag index

# Watch for changes
coderag watch
```

### 3. Search Your Code
```bash
# Semantic search
coderag search "authentication middleware"

# Symbol search
coderag search --symbol "processPayment" --kind function
```

### 4. Start MCP Server (for LLMs)
```bash
# Stdio mode (for Claude Desktop)
coderag serve

# HTTP mode (for remote access)
coderag serve --transport http --port 3000
```

## 🔧 Configuration

Create `.coderag/config.toml` in your project:

```toml
[indexer]
# Enable parallel indexing for speed
parallel_threads = null  # Auto-detect CPU cores
file_batch_size = 100
max_concurrent_files = 50

[embeddings]
# Choose your embedding provider
provider = "fastembed"  # or "openai"

[embeddings.providers.fastembed]
model = "nomic-embed-text-v1.5"
batch_size = 32

[embeddings.providers.openai]
api_key = "${OPENAI_API_KEY}"
model = "text-embedding-3-small"

[search]
mode = "hybrid"  # "vector", "bm25", or "hybrid"
vector_weight = 0.7
bm25_weight = 0.3
include_file_header = true
```

See [Configuration Guide](docs/CONFIGURATION.md) for all options.

## 🤝 Integration with Claude

Add to your Claude Desktop configuration:

```json
{
  "mcpServers": {
    "coderag": {
      "command": "coderag",
      "args": ["serve"],
      "env": {
        "OPENAI_API_KEY": "sk-..."  // If using OpenAI
      }
    }
  }
}
```

### Available MCP Tools

| Tool | Description | Example |
|------|-------------|---------|
| `search` | Semantic code search | Find authentication logic |
| `list_files` | Browse indexed files | List all TypeScript files |
| `get_file` | Read file contents | View main.rs |
| `find_symbol` | Search symbols by name | Find class UserService |
| `list_symbols` | List file symbols | Show all functions in file |
| `find_references` | Find symbol usage | Where is User class used |

## 📊 Performance

### Indexing Speed
- **Sequential**: 100 files/sec
- **Parallel (8 cores)**: 300+ files/sec
- **With OpenAI**: 150 files/sec

### Search Latency
- **Vector search**: <50ms
- **Symbol search**: <10ms
- **Hybrid search**: <100ms

### Memory Usage
- Small projects (<1k files): ~100MB
- Medium projects (1-5k files): ~300MB
- Large projects (10k+ files): ~500MB

See [Performance Documentation](docs/PERFORMANCE.md) for benchmarks.

## 🧪 Testing

CodeRAG includes comprehensive test coverage:

```bash
# Run all tests
cargo test

# Run benchmarks
cargo bench

# Check coverage
cargo tarpaulin --out Html
```

- 150+ unit and integration tests
- Language-specific test suites
- Performance benchmarks
- 85%+ code coverage

See [Testing Documentation](docs/TESTING.md) for details.

## 📚 Documentation

- [Configuration Guide](docs/CONFIGURATION.md) - All configuration options
- [MCP Tools Documentation](docs/MCP_TOOLS.md) - Using with LLMs
- [Language Support](docs/LANGUAGE_SUPPORT.md) - Supported languages and features
- [Performance Guide](docs/PERFORMANCE.md) - Optimization and benchmarks
- [Testing Guide](docs/TESTING.md) - Test infrastructure
- [Migration Guide](docs/MIGRATION_GUIDE.md) - Upgrading from older versions
- [API Documentation](https://docs.rs/coderag) - Rust API reference

## 🛠️ CLI Commands

### Core Commands
```bash
coderag init                   # Initialize in current directory
coderag index [--force]         # Index codebase
coderag search <query>          # Search for code
coderag watch                   # Auto-reindex on changes
coderag serve                   # Start MCP server
coderag web [--port 8080]       # Launch web interface
coderag stats                   # Show index statistics
```

### Project Management
```bash
coderag projects list           # List all projects
coderag projects add <name>     # Add current directory
coderag projects switch <name>  # Switch active project
coderag projects remove <name>  # Remove from registry
```

## 🏗️ Architecture

```
CodeRAG
├── Indexer (AST-based chunking)
│   ├── Tree-sitter parsers
│   ├── Parallel processing
│   └── Symbol extraction
├── Embeddings
│   ├── FastEmbed (local)
│   └── OpenAI API
├── Storage
│   ├── LanceDB (vectors)
│   └── Tantivy (BM25)
├── Search
│   ├── Vector search
│   ├── Keyword search
│   └── Hybrid (RRF fusion)
└── MCP Server
    ├── Stdio transport
    └── HTTP/SSE transport
```

## 🎯 Use Cases

### For Developers
- **Code exploration**: Understand unfamiliar codebases
- **Refactoring**: Find similar patterns to refactor
- **Documentation**: Generate docs from code understanding
- **Code review**: Navigate and understand changes

### For AI/LLMs
- **Context retrieval**: Provide relevant code to LLMs
- **Code generation**: Understand existing patterns
- **Question answering**: Answer questions about codebase
- **Automated analysis**: Systematic code exploration

## 🔄 Recent Updates

### Latest Release (Unreleased)
- ✅ OpenAI embedding support
- ✅ 3-5x faster parallel indexing
- ✅ Symbol search MCP tools
- ✅ C/C++ language support
- ✅ File header injection
- ✅ Smart batch detection

See [CHANGELOG.md](CHANGELOG.md) for full history.

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup
```bash
git clone https://github.com/nolood/coderag.git
cd coderag
cargo build
cargo test
```

### Areas for Contribution
- Additional language support
- Embedding model integrations
- Search algorithm improvements
- UI/UX enhancements
- Documentation improvements

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- [Tree-sitter](https://tree-sitter.github.io/) for AST parsing
- [LanceDB](https://lancedb.com/) for vector storage
- [Tantivy](https://github.com/quickwit-oss/tantivy) for full-text search
- [FastEmbed-rs](https://github.com/Anush008/fastembed-rs) for embeddings
- [MCP](https://modelcontextprotocol.io/) for LLM integration

## 📞 Support

- [GitHub Issues](https://github.com/nolood/coderag/issues) - Bug reports and features
- [Discussions](https://github.com/nolood/coderag/discussions) - Questions and ideas
- [Discord](https://discord.gg/coderag) - Community chat

---

<p align="center">Made with ❤️ by <a href="https://github.com/nolood">@nolood</a></p>
<p align="center">⭐ Star us on GitHub!</p>