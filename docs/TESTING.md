# Testing Documentation

CodeRAG maintains comprehensive test coverage with 150+ tests across unit, integration, and benchmark suites. This document details the testing infrastructure, strategies, and guidelines.

## Test Overview

### Test Statistics
```
Total Tests: 150+
Unit Tests: 100+
Integration Tests: 30+
Benchmark Tests: 20+
Code Coverage: 85%+
```

### Test Categories

| Category | Count | Purpose | Runtime |
|----------|-------|---------|---------|
| Unit Tests | 100+ | Component isolation | <1s |
| Integration Tests | 30+ | End-to-end flows | 5-10s |
| Language Tests | 16 | Per-language chunking | 2s |
| Storage Tests | 15 | Database operations | 3s |
| Search Tests | 20 | Search accuracy | 2s |
| Benchmark Tests | 20 | Performance metrics | 30s |

## Running Tests

### All Tests
```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run in parallel
cargo test -- --test-threads=8

# Run with coverage
cargo tarpaulin --out Html
```

### Specific Test Categories
```bash
# Unit tests only
cargo test --lib

# Integration tests only
cargo test --test '*'

# Benchmarks
cargo bench

# Doc tests
cargo test --doc
```

### Individual Test Modules
```bash
# Storage tests
cargo test storage::

# Chunking tests
cargo test indexer::chunking::

# Search tests
cargo test search::
```

## Test Structure

### Directory Layout
```
tests/
├── integration/
│   ├── indexing_test.rs      # End-to-end indexing
│   ├── search_test.rs         # Search functionality
│   ├── mcp_test.rs           # MCP server tests
│   └── watch_test.rs         # File watcher tests
├── fixtures/
│   ├── languages/
│   │   ├── rust/            # Rust test files
│   │   ├── python/          # Python test files
│   │   ├── typescript/      # TypeScript test files
│   │   └── ...
│   └── projects/
│       ├── small/           # Small test project
│       ├── medium/          # Medium test project
│       └── large/           # Large test project
└── benchmarks/
    ├── indexing_bench.rs     # Indexing performance
    ├── search_bench.rs       # Search performance
    └── memory_bench.rs       # Memory usage
```

## Unit Tests

### Storage Layer Tests

```rust
#[cfg(test)]
mod storage_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_index() {
        let storage = Storage::new_temp().await.unwrap();
        assert!(storage.is_empty().await);
    }

    #[tokio::test]
    async fn test_add_chunks() {
        let storage = Storage::new_temp().await.unwrap();
        let chunks = vec![
            Chunk::new("test content", "file.rs", 1..10),
        ];
        storage.add_chunks(chunks).await.unwrap();
        assert_eq!(storage.count_chunks().await, 1);
    }

    #[tokio::test]
    async fn test_vector_search() {
        let storage = Storage::new_temp().await.unwrap();
        // Add test data
        let results = storage.vector_search("query", 10).await.unwrap();
        assert!(!results.is_empty());
    }
}
```

### Chunking Tests

```rust
#[cfg(test)]
mod chunking_tests {
    use super::*;

    #[test]
    fn test_rust_function_chunking() {
        let code = r#"
            fn process_data(input: &str) -> Result<String> {
                // Function body
                Ok(input.to_string())
            }
        "#;

        let chunks = chunk_rust_code(code);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("process_data"));
    }

    #[test]
    fn test_chunk_size_limits() {
        let large_code = "x".repeat(10000);
        let chunks = chunk_with_size_limit(&large_code, 512);

        for chunk in &chunks {
            assert!(chunk.token_count <= 1500); // max_chunk_tokens
            assert!(chunk.token_count >= 50);   // min_chunk_tokens
        }
    }
}
```

### Symbol Extraction Tests

```rust
#[cfg(test)]
mod symbol_tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols() {
        let code = r#"
            struct User {
                name: String,
            }

            impl User {
                fn new(name: String) -> Self {
                    User { name }
                }
            }
        "#;

        let symbols = extract_symbols(code, Language::Rust);
        assert_eq!(symbols.len(), 3); // User, User::new, name field

        let struct_symbol = symbols.iter()
            .find(|s| s.name == "User" && s.kind == SymbolKind::Struct)
            .unwrap();
        assert_eq!(struct_symbol.line, 2);
    }
}
```

## Integration Tests

### End-to-End Indexing Test

```rust
// tests/integration/indexing_test.rs
use coderag::commands::index;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_indexing_flow() {
    // Create test project
    let temp_dir = TempDir::new().unwrap();
    let project_path = temp_dir.path();

    // Add test files
    create_test_rust_file(project_path, "main.rs");
    create_test_python_file(project_path, "app.py");

    // Initialize and index
    coderag::commands::init(project_path).await.unwrap();
    let stats = coderag::commands::index(project_path).await.unwrap();

    // Verify results
    assert_eq!(stats.files_indexed, 2);
    assert!(stats.chunks_created > 0);
    assert!(stats.symbols_extracted > 0);
}
```

### Search Integration Test

```rust
#[tokio::test]
async fn test_search_accuracy() {
    let project = setup_test_project().await;

    // Test semantic search
    let results = project.search("authentication logic", 10).await.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].score > 0.7);

    // Test keyword search
    let results = project.search("getUserById", 10).await.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].content.contains("getUserById"));

    // Test hybrid search
    let results = project.hybrid_search("async database connection", 10).await.unwrap();
    assert!(!results.is_empty());
}
```

### MCP Server Test

```rust
#[tokio::test]
async fn test_mcp_tools() {
    let server = setup_mcp_server().await;

    // Test search tool
    let response = server.call_tool("search", json!({
        "query": "test function"
    })).await.unwrap();
    assert!(response["results"].is_array());

    // Test find_symbol tool
    let response = server.call_tool("find_symbol", json!({
        "query": "MyClass",
        "kind": "class"
    })).await.unwrap();
    assert!(response["symbols"].is_array());

    // Test list_files tool
    let response = server.call_tool("list_files", json!({
        "pattern": "*.rs"
    })).await.unwrap();
    assert!(!response["files"].as_array().unwrap().is_empty());
}
```

## Language-Specific Tests

### Test Coverage by Language

Each language has comprehensive test coverage:

```rust
// tests/fixtures/languages/test_all_languages.rs
#[test]
fn test_all_language_chunking() {
    let languages = vec![
        ("rust", "sample.rs"),
        ("python", "sample.py"),
        ("typescript", "sample.ts"),
        ("javascript", "sample.js"),
        ("go", "sample.go"),
        ("java", "Sample.java"),
        ("c", "sample.c"),
        ("cpp", "sample.cpp"),
    ];

    for (lang, file) in languages {
        let path = format!("tests/fixtures/languages/{}/{}", lang, file);
        let content = std::fs::read_to_string(&path).unwrap();

        let chunks = chunk_code(&content, Language::from_str(lang).unwrap());
        assert!(!chunks.is_empty(), "Failed to chunk {} code", lang);

        // Verify chunk quality
        for chunk in &chunks {
            assert!(chunk.is_valid_semantic_unit());
            assert!(chunk.preserves_context());
        }
    }
}
```

### Language Test Files

Each language has test files covering various constructs:

```python
# tests/fixtures/languages/python/sample.py
"""Test file for Python chunking"""

import asyncio
from typing import List, Optional

class DataProcessor:
    """Process data with various methods."""

    def __init__(self, config: dict):
        self.config = config

    async def process_async(self, data: List[str]) -> List[str]:
        """Async processing method."""
        results = []
        for item in data:
            result = await self._process_item(item)
            results.append(result)
        return results

    def process_sync(self, data: List[str]) -> List[str]:
        """Sync processing method."""
        return [self._transform(item) for item in data]

@dataclass
class Config:
    timeout: int = 30
    retries: int = 3
```

## Benchmark Tests

### Indexing Performance Benchmark

```rust
// tests/benchmarks/indexing_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_parallel_indexing(c: &mut Criterion) {
    let project = setup_large_project(); // 1000+ files

    c.bench_function("parallel_index_1000_files", |b| {
        b.iter(|| {
            black_box(index_parallel(&project, 8))
        });
    });

    c.bench_function("sequential_index_1000_files", |b| {
        b.iter(|| {
            black_box(index_sequential(&project))
        });
    });
}

criterion_group!(benches, bench_parallel_indexing);
criterion_main!(benches);
```

### Search Performance Benchmark

```rust
fn bench_search_performance(c: &mut Criterion) {
    let index = setup_test_index(); // Pre-built index

    let mut group = c.benchmark_group("search");

    group.bench_function("vector_search", |b| {
        b.iter(|| index.vector_search("test query", 10))
    });

    group.bench_function("bm25_search", |b| {
        b.iter(|| index.bm25_search("test query", 10))
    });

    group.bench_function("hybrid_search", |b| {
        b.iter(|| index.hybrid_search("test query", 10))
    });

    group.bench_function("symbol_search", |b| {
        b.iter(|| index.find_symbol("TestClass", SymbolKind::Class))
    });

    group.finish();
}
```

### Memory Usage Benchmark

```rust
fn bench_memory_usage(c: &mut Criterion) {
    c.bench_function("index_memory_1000_files", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::new(0, 0);

            for _ in 0..iters {
                let start = Instant::now();
                let before = get_memory_usage();

                let _index = create_index_1000_files();

                let after = get_memory_usage();
                total += start.elapsed();

                println!("Memory used: {} MB", (after - before) / 1_048_576);
            }

            total
        });
    });
}
```

## Test Data Generation

### Synthetic Test Data

```rust
pub fn generate_test_project(size: ProjectSize) -> TestProject {
    match size {
        ProjectSize::Small => {
            // 10-50 files, simple structure
            TestProject::new()
                .add_rust_files(10)
                .add_python_files(5)
                .with_avg_file_lines(100)
        },
        ProjectSize::Medium => {
            // 100-500 files, nested structure
            TestProject::new()
                .add_rust_files(50)
                .add_typescript_files(100)
                .add_python_files(30)
                .with_avg_file_lines(500)
                .with_nested_depth(3)
        },
        ProjectSize::Large => {
            // 1000+ files, complex structure
            TestProject::new()
                .add_mixed_language_files(1000)
                .with_avg_file_lines(1000)
                .with_nested_depth(5)
                .with_binary_files(100)
        }
    }
}
```

### Real-World Test Projects

```toml
# tests/fixtures/projects/real_world.toml
[[projects]]
name = "mini-redis"
url = "https://github.com/tokio-rs/mini-redis"
language = "rust"
files = 50
test_queries = [
    "connection handling",
    "command parsing",
    "async runtime"
]

[[projects]]
name = "express-sample"
url = "https://github.com/expressjs/express/examples"
language = "javascript"
files = 30
test_queries = [
    "middleware",
    "routing",
    "error handling"
]
```

## Test Utilities

### Test Helpers

```rust
// src/test_utils.rs
pub mod test_utils {
    use tempfile::TempDir;

    pub async fn setup_test_index() -> Index {
        let dir = TempDir::new().unwrap();
        let index = Index::create(dir.path()).await.unwrap();

        // Add standard test data
        index.add_test_chunks().await.unwrap();
        index.build().await.unwrap();

        index
    }

    pub fn assert_search_quality(results: &[SearchResult], expected_top: &str) {
        assert!(!results.is_empty());
        assert!(results[0].score > 0.7);
        assert!(results[0].content.contains(expected_top));
    }

    pub fn create_test_file(lang: Language, complexity: Complexity) -> String {
        match (lang, complexity) {
            (Language::Rust, Complexity::Simple) => {
                include_str!("fixtures/simple.rs").to_string()
            },
            // ... more combinations
        }
    }
}
```

### Assertion Macros

```rust
#[macro_export]
macro_rules! assert_chunks_valid {
    ($chunks:expr) => {
        for chunk in $chunks {
            assert!(chunk.start_line > 0);
            assert!(chunk.end_line >= chunk.start_line);
            assert!(!chunk.content.is_empty());
            assert!(chunk.token_count > 0);
        }
    };
}

#[macro_export]
macro_rules! assert_symbol_found {
    ($symbols:expr, $name:expr, $kind:expr) => {
        assert!(
            $symbols.iter().any(|s| s.name == $name && s.kind == $kind),
            "Symbol {} of kind {:?} not found", $name, $kind
        );
    };
}
```

## Continuous Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/test.yml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true

      - name: Cache dependencies
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Run tests
        run: |
          cargo test --all-features
          cargo test --doc

      - name: Run benchmarks
        run: cargo bench --no-run

      - name: Check coverage
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --out Xml

      - name: Upload coverage
        uses: codecov/codecov-action@v3
```

## Test Coverage

### Current Coverage Report
```
src/storage/mod.rs      92%
src/indexer/chunking.rs 88%
src/search/vector.rs    85%
src/search/hybrid.rs    83%
src/embeddings/mod.rs   90%
src/mcp/tools.rs        78%
src/watcher/mod.rs      75%
```

### Coverage Goals
- Overall: 85%+ (achieved)
- Core modules: 90%+
- New features: 80%+
- Edge cases: 70%+

## Writing Tests

### Test Guidelines

1. **Test Naming**
   ```rust
   // Good: Descriptive and specific
   #[test]
   fn test_chunk_large_rust_function_with_generics() { }

   // Bad: Too generic
   #[test]
   fn test_chunk() { }
   ```

2. **Test Independence**
   ```rust
   // Each test should be independent
   #[tokio::test]
   async fn test_feature() {
       let storage = Storage::new_temp().await.unwrap(); // Fresh state
       // ... test logic
   }
   ```

3. **Test Coverage**
   - Happy path
   - Edge cases
   - Error conditions
   - Performance characteristics

4. **Test Data**
   ```rust
   // Use fixtures for consistency
   let test_code = include_str!("fixtures/complex_function.rs");

   // Or generate for variety
   let test_code = generate_test_code(Complexity::High);
   ```

## Debugging Tests

### Running with Output
```bash
# Show println! output
cargo test -- --nocapture

# Run single test with backtrace
RUST_BACKTRACE=1 cargo test test_name

# Run with logging
RUST_LOG=debug cargo test
```

### Test Isolation
```bash
# Run tests sequentially to debug race conditions
cargo test -- --test-threads=1

# Run specific test in isolation
cargo test test_module::test_function -- --exact
```

## Performance Testing

### Load Testing
```rust
#[tokio::test]
async fn test_concurrent_searches() {
    let index = setup_test_index().await;

    let handles: Vec<_> = (0..100)
        .map(|i| {
            let index = index.clone();
            tokio::spawn(async move {
                index.search(&format!("query {}", i), 10).await
            })
        })
        .collect();

    let results = futures::future::join_all(handles).await;
    assert!(results.iter().all(|r| r.is_ok()));
}
```

### Stress Testing
```rust
#[tokio::test]
async fn test_large_file_handling() {
    let huge_file = "x".repeat(10_000_000); // 10MB file
    let result = chunk_code(&huge_file, Language::Text).await;
    assert!(result.is_ok());
    assert!(!result.unwrap().is_empty());
}
```