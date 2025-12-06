# CodeRAG Test Infrastructure Research - Phase 1

## Executive Summary

This research documents the existing test patterns and infrastructure in the CodeRAG codebase. The project currently uses unit tests exclusively, with no integration tests found. Tests are co-located with source code using Rust's `#[cfg(test)]` convention.

---

## 1. Existing Test Structure and Patterns

### Test Location Pattern
- **Pattern**: Tests are embedded in source files using `#[cfg(test)]` module blocks
- **No dedicated test directory**: Unlike many Rust projects, CodeRAG doesn't have a separate `tests/` directory
- **Co-location**: Test modules are placed at the end of the source file they test

### Test Module Examples

#### Pattern 1: Basic Unit Tests (Most Common)
**File**: `/home/nolood/general/coderag/src/watcher/debouncer.rs` (lines 72-99)
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_type_display() {
        assert_eq!(format!("{}", ChangeType::Created), "created");
        // ...
    }
}
```

**Characteristics**:
- Simple assertions using `assert!`, `assert_eq!`
- Single responsibility per test
- Clear test names describing behavior
- Uses `super::*` to import parent module items

#### Pattern 2: Tests with Temporary Directories
**File**: `/home/nolood/general/coderag/src/config.rs` (lines 367-405)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_save_and_load_config() {
        let dir = tempdir().unwrap();
        let config = Config::default();

        config.save(dir.path()).unwrap();
        let loaded = Config::load(dir.path()).unwrap();

        assert_eq!(config.indexer.extensions, loaded.indexer.extensions);
    }
}
```

**Characteristics**:
- Uses `tempfile::tempdir()` for creating temporary directories
- Tests file I/O operations
- Automatic cleanup via tempdir scope
- Straightforward error handling with `.unwrap()`

#### Pattern 3: Tests with Complex Setup
**File**: `/home/nolood/general/coderag/src/search/bm25.rs` (lines 337-406)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_chunk(id: &str, content: &str, file_path: &str) -> IndexedChunk {
        IndexedChunk {
            id: id.to_string(),
            content: content.to_string(),
            file_path: file_path.to_string(),
            // ... other fields
        }
    }

    #[test]
    fn test_bm25_add_and_search() {
        let dir = tempdir().unwrap();
        let mut index = Bm25Index::new(dir.path()).unwrap();

        let chunks = vec![
            create_test_chunk("1", "fn hello_world() { ... }", "src/main.rs"),
            create_test_chunk("2", "fn goodbye_world() { ... }", "src/lib.rs"),
        ];

        index.add_chunks(&chunks).unwrap();
        index.commit().unwrap();
        // ... assertions
    }
}
```

**Characteristics**:
- Helper functions for test data creation
- Tests involving file system operations
- Tests for complex state changes
- Good separation of setup and verification

#### Pattern 4: Tests with Ignored Markers
**File**: `/home/nolood/general/coderag/src/embeddings/fastembed.rs` (lines 114-179)
```rust
#[test]
#[ignore] // Requires model download
fn test_embed_texts() {
    let config = test_config();
    let generator = EmbeddingGenerator::new(&config).unwrap();
    // ... test code
}

fn test_config() -> EmbeddingsConfig {
    EmbeddingsConfig {
        model: "all-MiniLM-L6-v2".to_string(), // Smaller, faster for tests
        batch_size: 32,
    }
}
```

**Characteristics**:
- Uses `#[ignore]` for tests with external dependencies
- Comments explaining why tests are ignored
- Helper functions for test configuration
- Differentiates between slow and fast tests

#### Pattern 5: Tests with Default Values
**File**: `/home/nolood/general/coderag/src/indexer/ast_chunker/parser_pool.rs` (lines 105-178)
```rust
#[test]
fn test_supported_languages() {
    let pool = ParserPool::new();
    let languages = pool.supported_languages();

    assert!(languages.contains(&"rust"));
    assert!(languages.contains(&"python"));
    // ... more assertions
}
```

**Characteristics**:
- Tests using default/newly created instances
- Multiple assertions in single test
- No mutable state required
- Direct verification of behavior

---

## 2. Common Test Utilities and Fixtures

### Dependency: `tempfile` Crate
**Usage**: Creating isolated temporary directories for tests
**Files using it**:
- `/home/nolood/general/coderag/src/config.rs`
- `/home/nolood/general/coderag/src/search/bm25.rs`
- `/home/nolood/general/coderag/src/registry/project.rs`
- `/home/nolood/general/coderag/src/registry/global.rs`

**Pattern**:
```rust
use tempfile::tempdir;

let dir = tempdir().unwrap();
let path = dir.path();
// Use path for file operations
// Automatically cleaned up when `dir` is dropped
```

### Fixture Helper Functions

#### Config Test Fixture
**File**: `/home/nolood/general/coderag/src/embeddings/fastembed.rs`
```rust
fn test_config() -> EmbeddingsConfig {
    EmbeddingsConfig {
        model: "all-MiniLM-L6-v2".to_string(),
        batch_size: 32,
    }
}
```

#### Chunk Test Fixture
**File**: `/home/nolood/general/coderag/src/search/bm25.rs`
```rust
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
```

#### Project Test Fixture
**File**: `/home/nolood/general/coderag/src/registry/global.rs`
```rust
fn create_test_project(name: &str, path: &Path) -> ProjectInfo {
    ProjectInfo::new(name.to_string(), path.to_path_buf())
}
```

### Test Configuration Pattern
**File**: `/home/nolood/general/coderag/src/config.rs`
```rust
#[test]
fn test_default_config() {
    let config = Config::default();
    assert!(config.indexer.extensions.contains(&"rs".to_string()));
    // Tests default values
}
```

---

## 3. Integration Test Examples (Current Status)

### NO DEDICATED INTEGRATION TESTS FOUND

**Key Finding**: The codebase currently has zero integration tests. All tests found are unit tests.

**Implication**: There is no:
- Separate `tests/` directory
- End-to-end test scenarios
- Tests combining multiple modules
- Fixtures that test complete workflows

**Example of missing integration scenario**:
Should test complete flow: Init → Index → Search → Verify Results

---

## 4. Test Coverage Analysis by Module

### High Coverage Modules
1. **Config Module** (`src/config.rs`)
   - Default configuration tests
   - Save/load configuration tests
   - Missing config handling
   - Tests: ~3

2. **Search Modules** (`src/search/bm25.rs`)
   - BM25 index creation
   - Add and search operations
   - Index clearing
   - Tests: ~3

3. **AST Chunker** (`src/indexer/ast_chunker/`)
   - Parser pool tests
   - Language detection tests
   - Token estimation tests
   - Extractor tests (Java, Go, Python, etc.)
   - Tests: ~20+

4. **Registry** (`src/registry/`)
   - Project info creation
   - Project stats tracking
   - Path existence checks
   - Tests: ~4

### Lower Coverage Modules
1. **Watcher** (`src/watcher/`)
   - Tests: ~2 (basic stats and config)
   - Missing: Full file watching workflows

2. **Embeddings** (`src/embeddings/`)
   - Tests: ~2 (basic model parsing, empty text)
   - Most tests ignored due to external dependencies

3. **MCP Server** (`src/mcp/server.rs`)
   - Tests: ~5 (request deserialization)
   - Missing: Full MCP message flow tests

4. **Web Module** (`src/web/`)
   - Tests: ~0
   - No unit tests found

5. **Commands** (`src/commands/`)
   - Tests: ~1 (metric snapshot)
   - Missing: Full command execution tests

### Storage Module Analysis
**File**: `/home/nolood/general/coderag/src/storage/lancedb.rs`

**Current Status**: NO TESTS
- No unit tests in the file
- No test fixtures
- Critical database operations untested

**What Should Be Tested**:
- `insert_chunks()` - vector insertion
- `search()` - vector similarity search
- `delete_by_file()` - file deletion
- `list_files()` - file listing with patterns
- `get_file_mtimes()` - modification time tracking
- `count_chunks()` - chunk counting
- `clear()` - database clearing

---

## 5. Test Dependencies

### Current Test Dependencies (from Cargo.toml observations)
1. **tempfile** - Most used for temporary directory creation
2. **Standard library** - assert macros, PathBuf
3. **serde_json** - JSON parsing in MCP tests
4. **tree_sitter** - Parser tests

### Missing/Desired Dependencies
1. **mockall** - Not found, but would be useful for mocking
2. **tokio::test** - For async tests (some async code untested)
3. **proptest** - Property-based testing not in use
4. **rstest** - Parameterized tests not in use

---

## 6. Test Execution Patterns

### Standard Execution
```bash
cargo test
```

### Ignored Tests
Tests marked with `#[ignore]` skip by default:
- Embedding tests (require model downloads)
- Any heavy I/O or network tests

### Run Ignored Tests
```bash
cargo test -- --ignored
```

---

## 7. Recommendations for Test Organization

### Phase 1: Immediate Improvements (Quick Wins)

#### 1.1 Add Storage Module Tests
**Priority**: HIGH
**Effort**: MEDIUM
**Impact**: Covers critical database operations

Create tests for:
- Basic CRUD operations
- Vector search functionality
- File path operations
- Edge cases (empty database, null values)

**Location**: Add `#[cfg(test)]` module at end of `src/storage/lancedb.rs`

#### 1.2 Create Test Fixtures Module
**Priority**: MEDIUM
**Effort**: MEDIUM
**Impact**: Improves test code reuse

Create `src/testing.rs`:
```rust
pub mod fixtures {
    pub fn create_test_chunk(id: &str) -> IndexedChunk { ... }
    pub fn create_test_config() -> Config { ... }
    pub fn create_test_project() -> ProjectInfo { ... }
}
```

**Benefits**:
- Centralized test data creation
- Reduced duplication across test files
- Easier maintenance

#### 1.3 Add Web Module Tests
**Priority**: MEDIUM
**Effort**: LOW
**Impact**: Validates API handlers

Test:
- Handler response formats
- Serialization/deserialization
- Error handling

#### 1.4 Async Test Harness
**Priority**: LOW
**Effort**: LOW
**Impact**: Enables testing async code

Add `#[tokio::test]` for async functions:
```rust
#[tokio::test]
async fn test_async_operation() {
    // async test code
}
```

### Phase 2: Integration Tests (Medium-term)

#### 2.1 Create `tests/` Directory
**Structure**:
```
tests/
├── common/
│   ├── mod.rs (shared test utilities)
│   └── fixtures.rs
├── integration_init.rs (test `coderag init`)
├── integration_index.rs (test `coderag index`)
├── integration_search.rs (test `coderag search`)
└── integration_full_workflow.rs
```

#### 2.2 Full Workflow Test
**Scenario**:
```rust
#[test]
fn test_init_index_search_workflow() {
    // 1. Initialize project
    // 2. Create test files
    // 3. Run indexer
    // 4. Execute search
    // 5. Verify results
}
```

#### 2.3 Multi-project Integration
**Scenario**:
```rust
#[test]
fn test_multi_project_management() {
    // Test adding multiple projects
    // Test switching between projects
    // Test isolated indices
}
```

### Phase 3: Advanced Testing (Long-term)

#### 3.1 Property-Based Testing
**Tools**: `proptest`
**Candidates**:
- Parser robustness (various code samples)
- Chunking algorithms
- Search result consistency

#### 3.2 Benchmarking
**Tools**: `criterion` or `bench`
**Focus Areas**:
- Indexing performance
- Search latency
- Embedding generation

#### 3.3 Performance Regression Testing
- Track metrics over time
- CI integration

---

## 8. Current Test Gaps

### Critical Gaps
| Module | Gap | Severity |
|--------|-----|----------|
| Storage (lancedb.rs) | No tests | CRITICAL |
| Web handlers | No tests | HIGH |
| Watcher | Minimal coverage | HIGH |
| Embeddings | Most tests ignored | MEDIUM |
| Commands | Minimal coverage | MEDIUM |

### Missing Test Scenarios
1. **Error handling paths** - Most tests only cover happy paths
2. **Edge cases** - Empty inputs, large inputs, special characters
3. **Concurrent operations** - Multi-threaded indexing scenarios
4. **File system errors** - Permission denied, disk full scenarios
5. **Database corruption** - Recovery scenarios
6. **Memory leaks** - Long-running processes

---

## 9. Test Infrastructure Health Metrics

### Current State
- **Total Test Files**: 0 dedicated test files
- **Total Unit Tests**: ~40-50 (embedded in source)
- **Total Integration Tests**: 0
- **Average Tests Per Module**: 1-5
- **Coverage Estimate**: 30-40% (rough estimate)

### Test Quality Indicators
- **Positive**: Clear naming conventions, proper use of fixtures
- **Negative**: No dedicated test utilities, no integration tests, sparse coverage in critical modules

---

## 10. File-by-File Test Status

### Modules WITH Tests
```
src/config.rs                          ✓ 3 tests
src/search/bm25.rs                     ✓ 3 tests
src/watcher/debouncer.rs               ✓ 2 tests
src/watcher/handler.rs                 ✓ 2 tests
src/watcher/mod.rs                     ✓ 1 test
src/mcp/server.rs                      ✓ 5 tests
src/mcp/http.rs                        ✓ 2 tests
src/commands/stats.rs                  ✓ 1 test
src/registry/project.rs                ✓ 4 tests
src/registry/global.rs                 ✓ 5 tests
src/embeddings/fastembed.rs            ✓ 5 tests (3 ignored)
src/indexer/ast_chunker/mod.rs         ✓ 3 tests
src/indexer/ast_chunker/parser_pool.rs ✓ 5 tests
src/indexer/ast_chunker/extractors/mod.rs        ✓ 2 tests
src/indexer/ast_chunker/extractors/java.rs       ✓ 2+ tests
src/indexer/ast_chunker/extractors/go.rs         ✓ 2+ tests
src/indexer/ast_chunker/extractors/python.rs     ✓ 3 tests
```

### Modules WITHOUT Tests
```
src/storage/lancedb.rs                 ✗ 0 tests (CRITICAL)
src/web/handlers.rs                    ✗ 0 tests
src/web/routes.rs                      ✗ 0 tests
src/web/state.rs                       ✗ 0 tests
src/web/mod.rs                         ✗ 0 tests
src/commands/index.rs                  ✗ 0 tests
src/commands/init.rs                   ✗ 0 tests
src/commands/search.rs                 ✗ 0 tests
src/commands/serve.rs                  ✗ 0 tests
src/commands/watch.rs                  ✗ 0 tests
src/commands/projects.rs               ✗ 0 tests
src/metrics.rs                         ✗ 0 tests (likely)
src/indexer/mod.rs                     ? Not checked
src/indexer/handler.rs                 ? Not checked
```

---

## 11. Best Practices Observed

### Positive Patterns
1. **Test Naming**: Clear, descriptive names (`test_bm25_add_and_search`)
2. **Module Organization**: Tests at end of files using `#[cfg(test)]`
3. **Fixture Reuse**: Helper functions like `create_test_chunk()`
4. **Temporary Resources**: Using `tempfile::tempdir()` for cleanup
5. **Single Responsibility**: One test per behavior
6. **Comments**: Explaining why tests are ignored

### Anti-patterns to Avoid
1. **Ignored Tests**: Many tests marked `#[ignore]` (slow/dependency-heavy)
2. **Mocking**: No mocking framework used (all integration-style tests)
3. **Test Data**: Hard-coded test data spread across multiple files
4. **Error Paths**: Happy-path testing only

---

## 12. Recommended Next Steps

### For Next Phase:
1. Add comprehensive storage module tests (critical)
2. Create centralized test fixtures module
3. Add integration test directory structure
4. Implement async test support
5. Document test conventions

### Tooling Setup:
```toml
[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["full"] }
mockall = "0.12"  # For mocking
rstest = "0.18"   # For parameterized tests
```

---

## Summary

CodeRAG has a decent foundation of unit tests focused on individual components, but lacks:
- Storage module testing (critical)
- Integration tests
- Web module tests
- Comprehensive error path coverage

The codebase uses sound testing practices where tests exist, making it straightforward to expand coverage systematically. The recommended approach is:

1. **Immediate**: Add storage tests (highest impact, critical module)
2. **Short-term**: Create integration test framework
3. **Medium-term**: Expand coverage to web and command modules
4. **Long-term**: Property-based and performance testing

This research provides the foundation for implementing a comprehensive testing strategy across the CodeRAG project.
