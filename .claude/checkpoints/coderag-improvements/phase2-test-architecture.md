# CodeRAG Integration Test Infrastructure Architecture - Phase 2

## Architecture Overview

This document outlines the complete test infrastructure architecture for CodeRAG, focusing on integration tests that validate end-to-end workflows, storage operations, and cross-module interactions. The architecture prioritizes testability, maintainability, and comprehensive coverage of critical paths.

### Key Design Decisions
1. **Test Isolation**: Each test runs in isolated environments using temporary directories
2. **Mock Embeddings**: Using deterministic mock embeddings to avoid API dependencies
3. **Fixture-Based Testing**: Reusable test data and scenarios across test suites
4. **Async-First**: All integration tests use `#[tokio::test]` for async operations
5. **Layered Testing**: Unit → Integration → End-to-End progression

---

## 1. Directory Structure

```
tests/
├── integration/                    # Integration test suites
│   ├── mod.rs                     # Module declarations and common imports
│   ├── workflow_tests.rs          # Complete workflows (init→index→search)
│   ├── storage_tests.rs           # LanceDB CRUD operations (CRITICAL)
│   ├── mcp_server_tests.rs        # MCP protocol and tool testing
│   ├── watch_mode_tests.rs        # File watching and auto-indexing
│   ├── chunking_tests.rs          # Per-language chunking validation
│   └── registry_tests.rs          # Multi-project management
│
├── fixtures/                       # Test data and sample code
│   ├── mod.rs                     # Fixture module declarations
│   ├── languages/                 # Language-specific test files
│   │   ├── rust/
│   │   │   ├── simple.rs         # Basic Rust patterns
│   │   │   ├── complex.rs        # Advanced Rust features
│   │   │   └── edge_cases.rs     # Unicode, macros, unsafe
│   │   ├── python/
│   │   │   ├── simple.py         # Basic Python
│   │   │   ├── async_code.py     # Async/await patterns
│   │   │   └── classes.py        # OOP patterns
│   │   ├── typescript/
│   │   │   ├── simple.ts         # Basic TypeScript
│   │   │   ├── react.tsx         # React components
│   │   │   └── types.d.ts        # Type definitions
│   │   ├── go/
│   │   │   ├── simple.go         # Basic Go
│   │   │   └── interfaces.go     # Interface patterns
│   │   └── cpp/
│   │       ├── simple.cpp        # Basic C++
│   │       └── templates.hpp     # Template metaprogramming
│   │
│   ├── projects/                  # Complete test projects
│   │   ├── minimal/              # Minimal valid project
│   │   ├── monorepo/             # Multi-language monorepo
│   │   └── large/                # Performance test project
│   │
│   └── data/                      # Test data files
│       ├── chunks.json            # Pre-computed chunk expectations
│       ├── embeddings.json        # Mock embedding vectors
│       └── search_results.json    # Expected search results
│
├── helpers/                        # Test infrastructure
│   ├── mod.rs                     # Helper module declarations
│   ├── harness.rs                # Main test harness
│   ├── mock_embeddings.rs        # Mock embedding generator
│   ├── assertions.rs             # Custom assertion utilities
│   ├── project_builder.rs        # Test project construction
│   └── database.rs               # Test database utilities
│
├── benchmarks/                     # Performance benchmarks
│   ├── indexing_bench.rs         # Indexing performance
│   ├── search_bench.rs           # Search performance
│   └── chunking_bench.rs         # Chunking performance
│
└── e2e/                           # End-to-end tests
    ├── cli_tests.rs               # Full CLI command tests
    ├── mcp_client_tests.rs        # MCP client interaction
    └── web_api_tests.rs          # HTTP API tests
```

### Directory Descriptions

#### `/tests/integration/`
Core integration test suites that test module interactions:
- **workflow_tests.rs**: Full pipeline validation (init → index → search → verify)
- **storage_tests.rs**: Critical LanceDB operations, vector storage, CRUD
- **mcp_server_tests.rs**: MCP protocol compliance, tool invocation
- **watch_mode_tests.rs**: File system monitoring, auto-reindexing
- **chunking_tests.rs**: Language-specific chunking quality validation
- **registry_tests.rs**: Multi-project scenarios, project switching

#### `/tests/fixtures/`
Reusable test data and sample code:
- **languages/**: One subdirectory per supported language with varying complexity
- **projects/**: Complete project structures for end-to-end testing
- **data/**: Pre-computed expected outputs for deterministic testing

#### `/tests/helpers/`
Test infrastructure and utilities:
- **harness.rs**: Central test setup/teardown, environment configuration
- **mock_embeddings.rs**: Deterministic embedding generation (no API calls)
- **assertions.rs**: Domain-specific assertions (chunk quality, search relevance)
- **project_builder.rs**: Fluent API for constructing test projects
- **database.rs**: Temporary database management, cleanup

---

## 2. Test Harness Design

### Core Test Harness (`tests/helpers/harness.rs`)

```rust
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use coderag::{Config, Storage, Registry};

/// Main test environment providing isolated execution context
pub struct TestHarness {
    /// Temporary directory for this test run
    temp_dir: TempDir,

    /// Test-specific configuration
    config: Config,

    /// Mock embedding provider
    embeddings: MockEmbeddings,

    /// In-memory or temporary LanceDB instance
    storage: Storage,

    /// Project registry
    registry: Registry,

    /// Captured logs for assertions
    logs: Vec<LogEntry>,
}

impl TestHarness {
    /// Create new test harness with defaults
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let config = Self::test_config(temp_dir.path());
        let embeddings = MockEmbeddings::deterministic();
        let storage = Storage::in_memory().await?;
        let registry = Registry::new(temp_dir.path())?;

        Ok(Self {
            temp_dir,
            config,
            embeddings,
            storage,
            registry,
            logs: Vec::new(),
        })
    }

    /// Builder pattern for custom configuration
    pub fn builder() -> TestHarnessBuilder {
        TestHarnessBuilder::default()
    }

    /// Get path to test directory
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Create a test project
    pub async fn create_project(&mut self, name: &str) -> Result<ProjectHandle> {
        let project_path = self.temp_dir.path().join(name);
        std::fs::create_dir_all(&project_path)?;

        // Initialize project
        let project = self.registry.create_project(name, &project_path)?;

        Ok(ProjectHandle {
            path: project_path,
            project,
            harness: self,
        })
    }

    /// Add test file to project
    pub fn add_file(&self, project: &Path, relative_path: &str, content: &str) -> Result<()> {
        let file_path = project.join(relative_path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file_path, content)?;
        Ok(())
    }

    /// Run indexing on project
    pub async fn index_project(&mut self, project: &Path) -> Result<IndexingResult> {
        // Implementation
    }

    /// Execute search query
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Implementation
    }

    /// Assert chunk exists with content
    pub fn assert_chunk_contains(&self, file_path: &str, content: &str) {
        // Implementation
    }

    /// Assert search result ordering
    pub fn assert_search_order(&self, results: &[SearchResult], expected_files: &[&str]) {
        // Implementation
    }

    /// Clean up resources (called automatically on drop)
    fn cleanup(&mut self) {
        // Cleanup implementation
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Builder for custom test harness configuration
pub struct TestHarnessBuilder {
    with_mock_embeddings: bool,
    with_persistent_storage: bool,
    with_logging: bool,
    custom_config: Option<Config>,
}

impl TestHarnessBuilder {
    pub fn with_mock_embeddings(mut self) -> Self {
        self.with_mock_embeddings = true;
        self
    }

    pub fn with_persistent_storage(mut self) -> Self {
        self.with_persistent_storage = true;
        self
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.custom_config = Some(config);
        self
    }

    pub async fn build(self) -> Result<TestHarness> {
        // Builder implementation
    }
}
```

### Mock Embeddings Provider (`tests/helpers/mock_embeddings.rs`)

```rust
use coderag::embeddings::{EmbeddingProvider, EmbeddingResult};

/// Mock embedding generator for deterministic testing
pub struct MockEmbeddings {
    mode: MockMode,
    dimension: usize,
}

pub enum MockMode {
    /// Deterministic vectors based on content hash
    Deterministic,

    /// Fixed vectors for all inputs
    Fixed(Vec<f32>),

    /// Sequential vectors for debugging
    Sequential,

    /// Simulate errors
    Error(String),
}

impl MockEmbeddings {
    pub fn deterministic() -> Self {
        Self {
            mode: MockMode::Deterministic,
            dimension: 768,
        }
    }

    pub fn fixed(vector: Vec<f32>) -> Self {
        Self {
            mode: MockMode::Fixed(vector),
            dimension: vector.len(),
        }
    }

    pub fn with_error(message: String) -> Self {
        Self {
            mode: MockMode::Error(message),
            dimension: 768,
        }
    }
}

impl EmbeddingProvider for MockEmbeddings {
    async fn embed_texts(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        match &self.mode {
            MockMode::Deterministic => {
                // Generate deterministic vectors based on text content
                texts.iter().map(|text| {
                    let hash = calculate_hash(text);
                    generate_vector_from_hash(hash, self.dimension)
                }).collect()
            },
            MockMode::Fixed(vector) => {
                Ok(vec![vector.clone(); texts.len()])
            },
            MockMode::Sequential => {
                // Generate sequential vectors for debugging
                texts.iter().enumerate().map(|(i, _)| {
                    generate_sequential_vector(i, self.dimension)
                }).collect()
            },
            MockMode::Error(msg) => {
                Err(EmbeddingError::new(msg))
            }
        }
    }
}

fn calculate_hash(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn generate_vector_from_hash(hash: u64, dimension: usize) -> Vec<f32> {
    let mut vector = Vec::with_capacity(dimension);
    let mut seed = hash;

    for _ in 0..dimension {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let value = ((seed / 65536) % 1000) as f32 / 1000.0;
        vector.push(value);
    }

    normalize_vector(&mut vector);
    vector
}

fn normalize_vector(vector: &mut Vec<f32>) {
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for v in vector.iter_mut() {
            *v /= magnitude;
        }
    }
}
```

### Project Builder (`tests/helpers/project_builder.rs`)

```rust
use std::path::Path;

/// Fluent API for building test projects
pub struct ProjectBuilder {
    name: String,
    files: Vec<(String, String)>,
    config: Option<Config>,
}

impl ProjectBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            files: Vec::new(),
            config: None,
        }
    }

    /// Add a source file to the project
    pub fn with_file(mut self, path: impl Into<String>, content: impl Into<String>) -> Self {
        self.files.push((path.into(), content.into()));
        self
    }

    /// Add multiple files from fixtures
    pub fn with_fixture(mut self, fixture_name: &str) -> Self {
        let files = load_fixture_files(fixture_name);
        self.files.extend(files);
        self
    }

    /// Set custom configuration
    pub fn with_config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the project in the specified directory
    pub async fn build_in(self, parent_dir: &Path) -> Result<Project> {
        let project_path = parent_dir.join(&self.name);
        std::fs::create_dir_all(&project_path)?;

        // Write all files
        for (relative_path, content) in self.files {
            let file_path = project_path.join(&relative_path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file_path, content)?;
        }

        // Write config if provided
        if let Some(config) = self.config {
            config.save(&project_path)?;
        }

        Ok(Project {
            name: self.name,
            path: project_path,
        })
    }
}

/// Common project templates
impl ProjectBuilder {
    pub fn rust_library() -> Self {
        Self::new("rust_lib")
            .with_file("Cargo.toml", include_str!("../fixtures/projects/rust_lib/Cargo.toml"))
            .with_file("src/lib.rs", include_str!("../fixtures/projects/rust_lib/src/lib.rs"))
            .with_file("src/main.rs", include_str!("../fixtures/projects/rust_lib/src/main.rs"))
    }

    pub fn python_package() -> Self {
        Self::new("python_pkg")
            .with_file("setup.py", include_str!("../fixtures/projects/python_pkg/setup.py"))
            .with_file("package/__init__.py", include_str!("../fixtures/projects/python_pkg/package/__init__.py"))
            .with_file("package/main.py", include_str!("../fixtures/projects/python_pkg/package/main.py"))
    }

    pub fn typescript_app() -> Self {
        Self::new("ts_app")
            .with_file("package.json", include_str!("../fixtures/projects/ts_app/package.json"))
            .with_file("tsconfig.json", include_str!("../fixtures/projects/ts_app/tsconfig.json"))
            .with_file("src/index.ts", include_str!("../fixtures/projects/ts_app/src/index.ts"))
    }

    pub fn monorepo() -> Self {
        Self::new("monorepo")
            .with_fixture("monorepo")
    }
}
```

---

## 3. Test Categories

### Workflow Tests (`tests/integration/workflow_tests.rs`)

```rust
use crate::helpers::{TestHarness, ProjectBuilder};

#[tokio::test]
async fn test_complete_workflow_init_index_search() {
    let mut harness = TestHarness::new().await.unwrap();

    // Step 1: Create and initialize project
    let project = ProjectBuilder::rust_library()
        .with_file("src/search.rs", r#"
            pub fn binary_search<T: Ord>(arr: &[T], target: &T) -> Option<usize> {
                let mut left = 0;
                let mut right = arr.len();
                while left < right {
                    let mid = left + (right - left) / 2;
                    match arr[mid].cmp(target) {
                        std::cmp::Ordering::Equal => return Some(mid),
                        std::cmp::Ordering::Less => left = mid + 1,
                        std::cmp::Ordering::Greater => right = mid,
                    }
                }
                None
            }
        "#)
        .build_in(harness.path())
        .await
        .unwrap();

    // Step 2: Initialize CodeRAG in project
    let init_result = harness.init_project(&project.path).await.unwrap();
    assert!(init_result.success);
    assert!(project.path.join(".coderag").exists());

    // Step 3: Index the project
    let index_result = harness.index_project(&project.path).await.unwrap();
    assert!(index_result.chunks_created > 0);
    assert_eq!(index_result.files_indexed, 4); // lib.rs, main.rs, search.rs, Cargo.toml

    // Step 4: Search for specific code
    let search_results = harness.search("binary search algorithm", 5).await.unwrap();
    assert!(!search_results.is_empty());
    assert_eq!(search_results[0].file_path, "src/search.rs");
    assert!(search_results[0].content.contains("binary_search"));

    // Step 5: Verify chunk quality
    harness.assert_chunk_contains("src/search.rs", "pub fn binary_search");
    harness.assert_chunk_contains("src/search.rs", "std::cmp::Ordering");

    // Step 6: Test incremental update
    harness.add_file(&project.path, "src/sort.rs", r#"
        pub fn quicksort<T: Ord>(arr: &mut [T]) {
            // Implementation
        }
    "#).unwrap();

    let update_result = harness.index_project(&project.path).await.unwrap();
    assert_eq!(update_result.files_added, 1);
    assert_eq!(update_result.files_modified, 0);
    assert_eq!(update_result.files_deleted, 0);
}

#[tokio::test]
async fn test_multi_language_workflow() {
    let mut harness = TestHarness::new().await.unwrap();

    // Create multi-language project
    let project = ProjectBuilder::new("polyglot")
        .with_fixture("languages/rust/simple.rs")
        .with_fixture("languages/python/simple.py")
        .with_fixture("languages/typescript/simple.ts")
        .build_in(harness.path())
        .await
        .unwrap();

    // Index all languages
    let result = harness.index_project(&project.path).await.unwrap();

    // Verify each language was chunked appropriately
    let chunks = harness.get_all_chunks().await.unwrap();

    let rust_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.language == Some("rust".to_string()))
        .collect();
    assert!(!rust_chunks.is_empty());

    let python_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.language == Some("python".to_string()))
        .collect();
    assert!(!python_chunks.is_empty());

    let ts_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.language == Some("typescript".to_string()))
        .collect();
    assert!(!ts_chunks.is_empty());
}
```

### Storage Tests (`tests/integration/storage_tests.rs`) - CRITICAL

```rust
use crate::helpers::TestHarness;
use coderag::storage::{IndexedChunk, ChunkMetadata};

#[tokio::test]
async fn test_storage_insert_and_retrieve() {
    let harness = TestHarness::new().await.unwrap();
    let storage = harness.storage();

    // Create test chunks
    let chunks = vec![
        create_test_chunk("chunk1", "fn main() { println!(\"Hello\"); }", "src/main.rs"),
        create_test_chunk("chunk2", "pub struct Config { ... }", "src/config.rs"),
        create_test_chunk("chunk3", "impl Display for Error { ... }", "src/error.rs"),
    ];

    // Insert chunks
    storage.insert_chunks(&chunks).await.unwrap();

    // Retrieve by ID
    let retrieved = storage.get_chunk("chunk1").await.unwrap();
    assert_eq!(retrieved.content, "fn main() { println!(\"Hello\"); }");

    // List all chunks
    let all_chunks = storage.list_chunks().await.unwrap();
    assert_eq!(all_chunks.len(), 3);
}

#[tokio::test]
async fn test_storage_vector_search() {
    let mut harness = TestHarness::builder()
        .with_mock_embeddings()
        .build()
        .await
        .unwrap();

    let storage = harness.storage();

    // Insert chunks with mock embeddings
    let chunks = vec![
        create_test_chunk("1", "binary search implementation", "algorithms.rs"),
        create_test_chunk("2", "linear search function", "search.rs"),
        create_test_chunk("3", "hash table lookup", "hash.rs"),
        create_test_chunk("4", "tree traversal algorithm", "tree.rs"),
    ];

    storage.insert_chunks(&chunks).await.unwrap();

    // Search for similar chunks
    let query_embedding = harness.embed_text("search algorithm").await.unwrap();
    let results = storage.search_vectors(&query_embedding, 3).await.unwrap();

    assert_eq!(results.len(), 3);
    assert!(results[0].score > results[1].score); // Ordered by relevance
    assert!(results[0].content.contains("search"));
}

#[tokio::test]
async fn test_storage_delete_by_file() {
    let harness = TestHarness::new().await.unwrap();
    let storage = harness.storage();

    // Insert chunks from multiple files
    let chunks = vec![
        create_test_chunk("1", "content1", "file1.rs"),
        create_test_chunk("2", "content2", "file1.rs"),
        create_test_chunk("3", "content3", "file2.rs"),
        create_test_chunk("4", "content4", "file2.rs"),
        create_test_chunk("5", "content5", "file3.rs"),
    ];

    storage.insert_chunks(&chunks).await.unwrap();

    // Delete chunks from file1.rs
    let deleted_count = storage.delete_by_file("file1.rs").await.unwrap();
    assert_eq!(deleted_count, 2);

    // Verify remaining chunks
    let remaining = storage.list_chunks().await.unwrap();
    assert_eq!(remaining.len(), 3);
    assert!(remaining.iter().all(|c| c.file_path != "file1.rs"));
}

#[tokio::test]
async fn test_storage_update_chunks() {
    let harness = TestHarness::new().await.unwrap();
    let storage = harness.storage();

    // Insert initial chunk
    let chunk = create_test_chunk("1", "original content", "file.rs");
    storage.insert_chunks(&[chunk]).await.unwrap();

    // Update chunk
    let updated_chunk = create_test_chunk("1", "updated content", "file.rs");
    storage.update_chunk(&updated_chunk).await.unwrap();

    // Verify update
    let retrieved = storage.get_chunk("1").await.unwrap();
    assert_eq!(retrieved.content, "updated content");
}

#[tokio::test]
async fn test_storage_file_mtimes() {
    let harness = TestHarness::new().await.unwrap();
    let storage = harness.storage();

    // Insert chunks with different mtimes
    let chunks = vec![
        create_test_chunk_with_mtime("1", "content1", "file1.rs", 1000),
        create_test_chunk_with_mtime("2", "content2", "file1.rs", 1000),
        create_test_chunk_with_mtime("3", "content3", "file2.rs", 2000),
    ];

    storage.insert_chunks(&chunks).await.unwrap();

    // Get file modification times
    let mtimes = storage.get_file_mtimes().await.unwrap();

    assert_eq!(mtimes.len(), 2);
    assert_eq!(mtimes.get("file1.rs"), Some(&1000));
    assert_eq!(mtimes.get("file2.rs"), Some(&2000));
}

#[tokio::test]
async fn test_storage_count_and_clear() {
    let harness = TestHarness::new().await.unwrap();
    let storage = harness.storage();

    // Insert chunks
    let chunks = vec![
        create_test_chunk("1", "content1", "file1.rs"),
        create_test_chunk("2", "content2", "file2.rs"),
        create_test_chunk("3", "content3", "file3.rs"),
    ];

    storage.insert_chunks(&chunks).await.unwrap();

    // Count chunks
    let count = storage.count_chunks().await.unwrap();
    assert_eq!(count, 3);

    // Clear all chunks
    storage.clear().await.unwrap();

    // Verify cleared
    let count_after = storage.count_chunks().await.unwrap();
    assert_eq!(count_after, 0);
}

#[tokio::test]
async fn test_storage_concurrent_operations() {
    let harness = TestHarness::new().await.unwrap();
    let storage = harness.storage();

    // Spawn multiple concurrent operations
    let handles: Vec<_> = (0..10).map(|i| {
        let storage = storage.clone();
        tokio::spawn(async move {
            let chunk = create_test_chunk(
                &format!("chunk_{}", i),
                &format!("content_{}", i),
                &format!("file_{}.rs", i),
            );
            storage.insert_chunks(&[chunk]).await
        })
    }).collect();

    // Wait for all operations
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // Verify all chunks inserted
    let count = storage.count_chunks().await.unwrap();
    assert_eq!(count, 10);
}

// Helper functions
fn create_test_chunk(id: &str, content: &str, file_path: &str) -> IndexedChunk {
    IndexedChunk {
        id: id.to_string(),
        content: content.to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 10,
        language: Some("rust".to_string()),
        vector: vec![0.0; 768],
        mtime: 0,
    }
}

fn create_test_chunk_with_mtime(id: &str, content: &str, file_path: &str, mtime: u64) -> IndexedChunk {
    let mut chunk = create_test_chunk(id, content, file_path);
    chunk.mtime = mtime;
    chunk
}
```

### MCP Server Tests (`tests/integration/mcp_server_tests.rs`)

```rust
use crate::helpers::TestHarness;
use coderag::mcp::{McpServer, McpRequest, McpResponse, ToolCall};

#[tokio::test]
async fn test_mcp_server_initialization() {
    let harness = TestHarness::new().await.unwrap();
    let server = McpServer::new(harness.storage()).await.unwrap();

    // Test initialization request
    let init_request = McpRequest::initialize();
    let response = server.handle_request(init_request).await.unwrap();

    match response {
        McpResponse::Initialize(init_response) => {
            assert_eq!(init_response.protocol_version, "1.0");
            assert!(init_response.tools.len() > 0);
        }
        _ => panic!("Expected initialize response"),
    }
}

#[tokio::test]
async fn test_mcp_search_tool() {
    let mut harness = TestHarness::new().await.unwrap();

    // Setup test data
    let project = ProjectBuilder::rust_library()
        .build_in(harness.path())
        .await
        .unwrap();
    harness.index_project(&project.path).await.unwrap();

    // Create MCP server
    let server = McpServer::new(harness.storage()).await.unwrap();

    // Test search tool
    let tool_call = ToolCall {
        name: "search".to_string(),
        arguments: json!({
            "query": "main function",
            "limit": 5
        }),
    };

    let request = McpRequest::tool_call(tool_call);
    let response = server.handle_request(request).await.unwrap();

    match response {
        McpResponse::ToolResult(result) => {
            assert!(result.success);
            let results = result.data.as_array().unwrap();
            assert!(!results.is_empty());
        }
        _ => panic!("Expected tool result"),
    }
}

#[tokio::test]
async fn test_mcp_list_files_tool() {
    let mut harness = TestHarness::new().await.unwrap();

    // Setup test data
    let project = ProjectBuilder::rust_library()
        .build_in(harness.path())
        .await
        .unwrap();
    harness.index_project(&project.path).await.unwrap();

    let server = McpServer::new(harness.storage()).await.unwrap();

    // Test list_files tool
    let tool_call = ToolCall {
        name: "list_files".to_string(),
        arguments: json!({
            "pattern": "*.rs"
        }),
    };

    let request = McpRequest::tool_call(tool_call);
    let response = server.handle_request(request).await.unwrap();

    match response {
        McpResponse::ToolResult(result) => {
            assert!(result.success);
            let files = result.data.as_array().unwrap();
            assert!(files.iter().all(|f| f.as_str().unwrap().ends_with(".rs")));
        }
        _ => panic!("Expected tool result"),
    }
}

#[tokio::test]
async fn test_mcp_get_file_tool() {
    let mut harness = TestHarness::new().await.unwrap();

    // Setup test data
    let test_content = "pub fn test() { /* test code */ }";
    harness.add_file(harness.path(), "test.rs", test_content).unwrap();

    let server = McpServer::new(harness.storage()).await.unwrap();

    // Test get_file tool
    let tool_call = ToolCall {
        name: "get_file".to_string(),
        arguments: json!({
            "path": "test.rs"
        }),
    };

    let request = McpRequest::tool_call(tool_call);
    let response = server.handle_request(request).await.unwrap();

    match response {
        McpResponse::ToolResult(result) => {
            assert!(result.success);
            assert_eq!(result.data["content"].as_str().unwrap(), test_content);
        }
        _ => panic!("Expected tool result"),
    }
}

#[tokio::test]
async fn test_mcp_protocol_compliance() {
    let harness = TestHarness::new().await.unwrap();
    let server = McpServer::new(harness.storage()).await.unwrap();

    // Test complete MCP conversation flow
    // 1. Initialize
    let init_response = server.handle_request(McpRequest::initialize()).await.unwrap();
    assert!(matches!(init_response, McpResponse::Initialize(_)));

    // 2. List available tools
    let tools_request = McpRequest::list_tools();
    let tools_response = server.handle_request(tools_request).await.unwrap();
    assert!(matches!(tools_response, McpResponse::ToolsList(_)));

    // 3. Execute a tool
    let tool_call = ToolCall {
        name: "search".to_string(),
        arguments: json!({"query": "test"}),
    };
    let tool_response = server.handle_request(McpRequest::tool_call(tool_call)).await.unwrap();
    assert!(matches!(tool_response, McpResponse::ToolResult(_)));

    // 4. Close connection
    let close_response = server.handle_request(McpRequest::close()).await.unwrap();
    assert!(matches!(close_response, McpResponse::Close));
}
```

### Watch Mode Tests (`tests/integration/watch_mode_tests.rs`)

```rust
use crate::helpers::TestHarness;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_watch_mode_file_creation() {
    let mut harness = TestHarness::new().await.unwrap();
    let project = harness.create_project("watched").await.unwrap();

    // Start watcher
    let watcher = harness.start_watcher(&project.path).await.unwrap();

    // Add new file
    harness.add_file(&project.path, "src/new.rs", "fn new_function() {}").unwrap();

    // Wait for debouncing
    sleep(Duration::from_millis(500)).await;

    // Verify file was indexed
    let chunks = harness.get_chunks_for_file("src/new.rs").await.unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks[0].content.contains("new_function"));

    watcher.stop().await.unwrap();
}

#[tokio::test]
async fn test_watch_mode_file_modification() {
    let mut harness = TestHarness::new().await.unwrap();
    let project = harness.create_project("watched").await.unwrap();

    // Create initial file
    harness.add_file(&project.path, "src/lib.rs", "fn old() {}").unwrap();
    harness.index_project(&project.path).await.unwrap();

    // Start watcher
    let watcher = harness.start_watcher(&project.path).await.unwrap();

    // Modify file
    harness.add_file(&project.path, "src/lib.rs", "fn updated() {}").unwrap();

    // Wait for debouncing
    sleep(Duration::from_millis(500)).await;

    // Verify file was re-indexed
    let chunks = harness.get_chunks_for_file("src/lib.rs").await.unwrap();
    assert!(chunks[0].content.contains("updated"));
    assert!(!chunks[0].content.contains("old"));

    watcher.stop().await.unwrap();
}

#[tokio::test]
async fn test_watch_mode_file_deletion() {
    let mut harness = TestHarness::new().await.unwrap();
    let project = harness.create_project("watched").await.unwrap();

    // Create and index file
    let file_path = project.path.join("src/temp.rs");
    std::fs::write(&file_path, "fn temp() {}").unwrap();
    harness.index_project(&project.path).await.unwrap();

    // Start watcher
    let watcher = harness.start_watcher(&project.path).await.unwrap();

    // Delete file
    std::fs::remove_file(&file_path).unwrap();

    // Wait for debouncing
    sleep(Duration::from_millis(500)).await;

    // Verify chunks were removed
    let chunks = harness.get_chunks_for_file("src/temp.rs").await.unwrap();
    assert!(chunks.is_empty());

    watcher.stop().await.unwrap();
}

#[tokio::test]
async fn test_watch_mode_debouncing() {
    let mut harness = TestHarness::new().await.unwrap();
    let project = harness.create_project("watched").await.unwrap();

    // Start watcher with event collection
    let watcher = harness.start_watcher_with_events(&project.path).await.unwrap();

    // Rapid file changes
    for i in 0..5 {
        harness.add_file(&project.path, "src/rapid.rs", &format!("fn version_{}() {{}}", i)).unwrap();
        sleep(Duration::from_millis(50)).await;
    }

    // Wait for debouncing to complete
    sleep(Duration::from_millis(500)).await;

    // Should only have one index operation after debouncing
    let events = watcher.get_events().await;
    let index_events: Vec<_> = events.iter()
        .filter(|e| matches!(e, WatchEvent::Indexed(_)))
        .collect();

    assert_eq!(index_events.len(), 1);

    watcher.stop().await.unwrap();
}
```

### Language-Specific Chunking Tests (`tests/integration/chunking_tests.rs`)

```rust
use crate::helpers::TestHarness;
use crate::fixtures::languages;

#[tokio::test]
async fn test_rust_chunking() {
    let harness = TestHarness::new().await.unwrap();

    // Load Rust fixture
    let rust_code = languages::rust::COMPLEX_CODE;
    let chunks = harness.chunk_code(rust_code, "rust").await.unwrap();

    // Verify chunking quality
    assert!(chunks.len() > 1);

    // Each chunk should be a complete syntactic unit
    for chunk in &chunks {
        assert!(is_valid_rust_chunk(&chunk.content));
    }

    // Verify important constructs are preserved
    assert!(chunks.iter().any(|c| c.content.contains("impl")));
    assert!(chunks.iter().any(|c| c.content.contains("pub fn")));
    assert!(chunks.iter().any(|c| c.content.contains("struct")));
}

#[tokio::test]
async fn test_python_chunking() {
    let harness = TestHarness::new().await.unwrap();

    // Load Python fixture with classes and async functions
    let python_code = languages::python::ASYNC_CODE;
    let chunks = harness.chunk_code(python_code, "python").await.unwrap();

    // Verify async functions are kept intact
    let async_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.content.contains("async def"))
        .collect();
    assert!(!async_chunks.is_empty());

    // Verify class methods are grouped appropriately
    let class_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.content.contains("class "))
        .collect();

    for chunk in class_chunks {
        // Class definition and at least constructor should be together
        assert!(chunk.content.contains("__init__"));
    }
}

#[tokio::test]
async fn test_typescript_react_chunking() {
    let harness = TestHarness::new().await.unwrap();

    // Load React component fixture
    let tsx_code = languages::typescript::REACT_COMPONENT;
    let chunks = harness.chunk_code(tsx_code, "typescript").await.unwrap();

    // Verify JSX is preserved
    assert!(chunks.iter().any(|c| c.content.contains("<div")));

    // Verify hooks are captured
    assert!(chunks.iter().any(|c| c.content.contains("useState")));
    assert!(chunks.iter().any(|c| c.content.contains("useEffect")));

    // Component should be in a single chunk if small enough
    let component_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.content.contains("export default"))
        .collect();
    assert_eq!(component_chunks.len(), 1);
}

#[tokio::test]
async fn test_go_interface_chunking() {
    let harness = TestHarness::new().await.unwrap();

    let go_code = languages::go::INTERFACES;
    let chunks = harness.chunk_code(go_code, "go").await.unwrap();

    // Interfaces should be kept intact
    let interface_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.content.contains("type") && c.content.contains("interface"))
        .collect();

    for chunk in interface_chunks {
        // Interface definition should be complete
        assert!(chunk.content.contains("{"));
        assert!(chunk.content.contains("}"));
    }
}

#[tokio::test]
async fn test_cpp_template_chunking() {
    let harness = TestHarness::new().await.unwrap();

    let cpp_code = languages::cpp::TEMPLATES;
    let chunks = harness.chunk_code(cpp_code, "cpp").await.unwrap();

    // Template definitions should be kept together
    let template_chunks: Vec<_> = chunks.iter()
        .filter(|c| c.content.contains("template"))
        .collect();

    for chunk in template_chunks {
        // Template parameter list should be with the definition
        assert!(chunk.content.contains("template<"));
        assert!(chunk.content.contains(">"));
    }
}

#[tokio::test]
async fn test_chunking_edge_cases() {
    let harness = TestHarness::new().await.unwrap();

    // Test empty file
    let chunks = harness.chunk_code("", "rust").await.unwrap();
    assert!(chunks.is_empty());

    // Test file with only comments
    let comment_only = "// This is a comment\n// Another comment\n/* Block comment */";
    let chunks = harness.chunk_code(comment_only, "rust").await.unwrap();
    assert_eq!(chunks.len(), 1);

    // Test very large function
    let large_function = format!("fn huge() {{\n{}\n}}", "    let x = 1;\n".repeat(1000));
    let chunks = harness.chunk_code(&large_function, "rust").await.unwrap();
    // Should split into multiple chunks
    assert!(chunks.len() > 1);

    // Test Unicode content
    let unicode_code = r#"
        fn 你好() {
            let emoji = "🦀";
            let japanese = "こんにちは";
        }
    "#;
    let chunks = harness.chunk_code(unicode_code, "rust").await.unwrap();
    assert!(chunks[0].content.contains("🦀"));
}

// Helper functions
fn is_valid_rust_chunk(content: &str) -> bool {
    // Basic validation that chunk is syntactically complete
    let open_braces = content.matches('{').count();
    let close_braces = content.matches('}').count();
    open_braces == close_braces
}
```

---

## 4. Fixture Organization

### Language Fixtures Structure

```
tests/fixtures/languages/
├── rust/
│   ├── simple.rs          # Basic functions and structs
│   ├── complex.rs         # Traits, generics, macros
│   └── edge_cases.rs      # Unicode, raw strings, unsafe
├── python/
│   ├── simple.py          # Functions and classes
│   ├── async_code.py      # Async/await patterns
│   └── decorators.py      # Complex decorators
├── typescript/
│   ├── simple.ts          # Basic TypeScript
│   ├── react.tsx          # React components
│   └── types.d.ts         # Type definitions
├── go/
│   ├── simple.go          # Basic Go
│   └── interfaces.go      # Interface patterns
└── cpp/
    ├── simple.cpp         # Basic C++
    └── templates.hpp      # Template metaprogramming
```

### Sample Fixture Files

#### `tests/fixtures/languages/rust/complex.rs`
```rust
use std::marker::PhantomData;

/// Generic trait with associated types
pub trait Container<T> {
    type Item;

    fn insert(&mut self, item: Self::Item);
    fn get(&self, index: usize) -> Option<&Self::Item>;
}

/// Complex generic struct
pub struct SmartBuffer<T, const N: usize>
where
    T: Clone + Default,
{
    data: [T; N],
    cursor: usize,
    _phantom: PhantomData<T>,
}

impl<T, const N: usize> SmartBuffer<T, N>
where
    T: Clone + Default,
{
    pub fn new() -> Self {
        Self {
            data: [T::default(); N],
            cursor: 0,
            _phantom: PhantomData,
        }
    }

    pub fn push(&mut self, item: T) -> Result<(), BufferError> {
        if self.cursor >= N {
            return Err(BufferError::Full);
        }
        self.data[self.cursor] = item;
        self.cursor += 1;
        Ok(())
    }
}

/// Macro for generating getters
macro_rules! generate_getters {
    ($($name:ident: $type:ty),*) => {
        $(
            pub fn $name(&self) -> &$type {
                &self.$name
            }
        )*
    };
}
```

#### `tests/fixtures/languages/python/async_code.py`
```python
import asyncio
from typing import List, Optional, Dict
from dataclasses import dataclass

@dataclass
class AsyncResult:
    """Result from async operation"""
    data: Dict[str, any]
    status: str
    timestamp: float

class AsyncProcessor:
    """Async data processor with connection pooling"""

    def __init__(self, max_connections: int = 10):
        self.max_connections = max_connections
        self._pool: List[asyncio.Queue] = []
        self._results_cache: Dict[str, AsyncResult] = {}

    async def process_batch(self, items: List[str]) -> List[AsyncResult]:
        """Process items in parallel with rate limiting"""
        semaphore = asyncio.Semaphore(self.max_connections)

        async def process_with_limit(item: str) -> AsyncResult:
            async with semaphore:
                return await self._process_single(item)

        tasks = [process_with_limit(item) for item in items]
        return await asyncio.gather(*tasks)

    async def _process_single(self, item: str) -> AsyncResult:
        """Process single item with caching"""
        if item in self._results_cache:
            return self._results_cache[item]

        # Simulate async work
        await asyncio.sleep(0.1)

        result = AsyncResult(
            data={"processed": item},
            status="complete",
            timestamp=asyncio.get_event_loop().time()
        )

        self._results_cache[item] = result
        return result

async def main():
    processor = AsyncProcessor(max_connections=5)
    items = [f"item_{i}" for i in range(100)]

    results = await processor.process_batch(items)
    print(f"Processed {len(results)} items")

if __name__ == "__main__":
    asyncio.run(main())
```

---

## 5. CI Integration

### GitHub Actions Configuration

```yaml
# .github/workflows/integration-tests.yml
name: Integration Tests

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    name: Run Integration Tests
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        rust: [stable, beta]

    steps:
      - uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: ${{ matrix.rust }}
          override: true
          components: rustfmt, clippy

      - name: Cache dependencies
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Run unit tests
        run: cargo test --lib --bins

      - name: Run integration tests
        run: cargo test --test '*' --features integration-tests

      - name: Run storage tests (critical)
        run: cargo test --test storage_tests

      - name: Run ignored tests (if not PR)
        if: github.event_name != 'pull_request'
        run: cargo test -- --ignored

      - name: Generate test coverage
        if: matrix.os == 'ubuntu-latest' && matrix.rust == 'stable'
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --out Xml --output-dir coverage

      - name: Upload coverage
        if: matrix.os == 'ubuntu-latest' && matrix.rust == 'stable'
        uses: codecov/codecov-action@v3
        with:
          files: ./coverage/cobertura.xml

  benchmark:
    name: Run Benchmarks
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'

    steps:
      - uses: actions/checkout@v3

      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true

      - name: Run benchmarks
        run: cargo bench --bench '*' -- --output-format bencher | tee output.txt

      - name: Store benchmark result
        uses: benchmark-action/github-action-benchmark@v1
        with:
          tool: 'cargo'
          output-file-path: output.txt
          github-token: ${{ secrets.GITHUB_TOKEN }}
          auto-push: true
```

### Test Execution Strategy

```toml
# Cargo.toml additions
[features]
default = []
integration-tests = ["tempfile", "tokio-test"]

[dev-dependencies]
# Test framework
tokio-test = "0.4"
tempfile = "3"
rstest = "0.18"
mockall = "0.12"

# Benchmarking
criterion = { version = "0.5", features = ["html_reports"] }

# Assertions
assert_fs = "1.0"
predicates = "3.0"

[[test]]
name = "integration"
path = "tests/integration/mod.rs"
required-features = ["integration-tests"]

[[bench]]
name = "indexing"
harness = false
```

---

## 6. Test Execution Commands

### Development Commands
```bash
# Run all tests
cargo test

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test integration

# Run specific test suite
cargo test --test storage_tests

# Run with verbose output
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored

# Run tests in parallel
cargo test -- --test-threads=4

# Run benchmarks
cargo bench

# Generate coverage report
cargo tarpaulin --out Html
```

### CI-Specific Commands
```bash
# Fast CI tests (no ignored)
cargo test --all --no-fail-fast

# Full test suite (with ignored)
cargo test --all -- --include-ignored

# Platform-specific tests
cargo test --target x86_64-pc-windows-msvc
cargo test --target x86_64-apple-darwin
cargo test --target x86_64-unknown-linux-gnu
```

---

## 7. Test Architecture Patterns

### Domain-Driven Test Organization
- Tests organized by bounded context (storage, indexing, search)
- Each test file focuses on a single aggregate or service
- Integration tests validate cross-boundary interactions

### Test Data Management
- **Fixtures**: Immutable test data in version control
- **Builders**: Dynamic test data construction
- **Mocks**: Deterministic external service simulation

### Assertion Strategy
- **Behavior Assertions**: Verify outcomes, not implementation
- **Property-Based**: Invariants that must hold across inputs
- **Snapshot Testing**: For complex output validation

### Performance Testing
- **Microbenchmarks**: Individual function performance
- **Integration Benchmarks**: End-to-end workflow timing
- **Regression Detection**: Automated performance tracking

---

## 8. Implementation Priorities

### Phase 1: Critical Path (Week 1)
1. ✅ Create directory structure
2. ✅ Implement TestHarness base
3. ✅ Add storage tests (CRITICAL - currently missing)
4. ✅ Basic workflow test (init→index→search)

### Phase 2: Core Coverage (Week 2)
1. Mock embedding provider
2. MCP server tests
3. Language-specific chunking tests
4. Watch mode tests

### Phase 3: Advanced Testing (Week 3)
1. Performance benchmarks
2. Multi-project tests
3. Error scenario coverage
4. CI/CD integration

### Phase 4: Maintenance (Ongoing)
1. Test documentation
2. Coverage monitoring
3. Performance regression tracking
4. Test refactoring

---

## Summary

This integration test architecture provides:

1. **Comprehensive Coverage**: Storage, workflows, MCP, languages
2. **Test Isolation**: Temporary environments, mock dependencies
3. **Maintainable Structure**: Clear organization, reusable components
4. **CI Integration**: Automated testing across platforms
5. **Performance Tracking**: Benchmarks with regression detection

The architecture emphasizes testing critical paths (especially storage operations which currently have no tests) while providing a scalable foundation for expanding test coverage. The use of mock embeddings ensures tests run quickly without external dependencies, while the fixture system provides realistic test scenarios.

Next steps:
1. Implement storage tests immediately (critical gap)
2. Create the TestHarness foundation
3. Add workflow integration tests
4. Integrate with CI/CD pipeline