# CodeRAG Installation Guide

## Overview

CodeRAG is a semantic code search tool with MCP (Model Context Protocol) support for Claude Code. With the new **zero-ceremony** feature, you only need to install and configure once - CodeRAG automatically indexes any project you work in.

## Quick Start

```bash
# 1. Install binary
sudo cp target/release/coderag /usr/local/bin/

# 2. Configure Claude Code MCP (global)
claude mcp add coderag -s user -- coderag serve

# 3. Done! Use from any project directory
cd ~/your-project
# Claude can now search your code via MCP
```

---

## Detailed Installation

### Step 1: Install the Binary

**Option A: System-wide installation (recommended)**
```bash
sudo cp target/release/coderag /usr/local/bin/
sudo chmod +x /usr/local/bin/coderag
```

**Option B: User installation**
```bash
mkdir -p ~/.local/bin
cp target/release/coderag ~/.local/bin/
chmod +x ~/.local/bin/coderag

# Add to PATH if not already (add to ~/.bashrc or ~/.zshrc)
export PATH="$HOME/.local/bin:$PATH"
```

**Option C: Build from source**
```bash
git clone https://github.com/your-repo/coderag.git
cd coderag
cargo build --release
sudo cp target/release/coderag /usr/local/bin/
```

### Step 2: Verify Installation

```bash
coderag --version
# Output: coderag 0.1.0

coderag --help
# Shows available commands
```

### Step 3: Configure Claude Code MCP

**Global MCP Configuration (recommended)**

This makes CodeRAG available in ALL your projects without any additional setup:

```bash
# Add as global MCP server
claude mcp add coderag -s user -- coderag serve
```

**Verify MCP is configured:**
```bash
claude mcp list
```

Expected output:
```
User settings (~/.config/claude/settings.json):
  - coderag: coderag serve
```

**Alternative: Manual configuration**

Edit `~/.config/claude/settings.json`:

```json
{
  "mcpServers": {
    "coderag": {
      "command": "coderag",
      "args": ["serve"]
    }
  }
}
```

Or with full path if not in PATH:
```json
{
  "mcpServers": {
    "coderag": {
      "command": "/usr/local/bin/coderag",
      "args": ["serve"]
    }
  }
}
```

---

## Usage

### Zero-Ceremony Mode (New!)

Just navigate to any project and use Claude - CodeRAG handles everything automatically:

```bash
cd ~/projects/my-rust-app
# Start Claude Code - MCP auto-indexes your project on first search
```

In Claude, you can now:
- Search code semantically
- Find implementations
- Explore codebase

**Project Detection:**
CodeRAG automatically detects project roots by looking for:
- `.git` - Git repositories
- `Cargo.toml` - Rust projects
- `package.json` - Node.js projects
- `pyproject.toml` / `setup.py` - Python projects
- `go.mod` - Go projects
- `pom.xml` / `build.gradle` - Java projects

### CLI Commands

```bash
# Check project status
coderag status

# Manual search (if needed)
coderag search "function that handles errors"

# Force re-index
coderag index --force

# Create local config (optional - for custom settings)
coderag init
```

### Storage Locations

**Global indexes (default for new projects):**
```
~/.local/share/coderag/indexes/{project-id}/
```

**Local indexes (projects with .coderag/):**
```
{project}/.coderag/index.lance
```

---

## Advanced Configuration

### Per-Project Configuration

Create `.coderag/config.toml` in your project for custom settings:

```toml
[indexer]
# File extensions to index
extensions = ["rs", "py", "js", "ts"]

# Patterns to ignore
ignore_patterns = ["**/node_modules/**", "**/target/**", "**/.git/**"]

# Chunking strategy: "line" or "ast"
chunker_strategy = "ast"

[embeddings]
# Model for embeddings
model = "nomic-embed-text-v1.5"
batch_size = 64

[search]
# Search mode: "hybrid", "vector", or "bm25"
mode = "hybrid"
default_limit = 10
```

### Environment Variables

```bash
# Custom data directory
export CODERAG_DATA_DIR=~/.coderag

# Logging level
export RUST_LOG=info
```

---

## Troubleshooting

### MCP not working

1. Check if coderag is in PATH:
   ```bash
   which coderag
   ```

2. Verify MCP configuration:
   ```bash
   claude mcp list
   ```

3. Test MCP server manually:
   ```bash
   coderag serve
   # Should start and wait for input
   ```

### Index not found

1. Check project detection:
   ```bash
   coderag status
   ```

2. Force re-index:
   ```bash
   coderag index --force
   ```

### First search is slow

The first search in a new project triggers auto-indexing, which includes:
1. Downloading the embedding model (~100MB, cached after first use)
2. Indexing all source files

Subsequent searches are fast (milliseconds).

### Clear global indexes

```bash
rm -rf ~/.local/share/coderag/indexes/
```

---

## MCP Tools Available

When configured as MCP server, CodeRAG provides these tools to Claude:

| Tool | Description |
|------|-------------|
| `search` | Semantic code search using natural language |
| `list_files` | List all indexed files with optional glob filtering |
| `get_file` | Get full content of a specific file |

---

## System Requirements

- **OS:** Linux (x86_64)
- **RAM:** 2GB+ (for embedding model)
- **Disk:** ~200MB for binary + ~100MB for model cache
- **Dependencies:** None (statically linked)

---

## Uninstallation

```bash
# Remove binary
sudo rm /usr/local/bin/coderag

# Remove MCP configuration
claude mcp remove coderag

# Remove data (optional)
rm -rf ~/.local/share/coderag/
```
