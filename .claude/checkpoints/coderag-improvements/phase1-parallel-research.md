# Phase 1: Parallel Processing Patterns for File Indexing - Research

**Date:** 2025-12-06
**Focus:** Analyzing current file indexing loops and integrating parallel processing patterns using Rayon and Tokio

---

## Executive Summary

This research document identifies opportunities to parallelize file indexing operations in CodeRAG. The current implementation processes files sequentially in loops. By leveraging Rayon for data-level parallelism and Tokio's task-spawning capabilities, we can achieve significant performance improvements in both batch indexing and watch mode operations.

---

## 1. Current File Processing Code Locations

### 1.1 Batch Indexing - `src/commands/index.rs`

**File Path:** `/home/nolood/general/coderag/src/commands/index.rs`

**Current Sequential Loop (Lines 47-103):**
```rust
for (idx, path) in files_to_index.iter().enumerate() {
    pb.set_position(idx as u64);

    // Read file content
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            debug!("Skipping file {:?}: {}", path, e);
            continue;
        }
    };

    // Get file mtime
    let mtime = get_file_mtime(path).unwrap_or(0);

    // Delete existing chunks for this file (for re-indexing)
    storage.delete_by_file(path).await?;

    // Chunk the file
    let chunks = chunker.chunk_file(path, &content);

    if chunks.is_empty() {
        continue;
    }

    // Prepare chunks for embedding
    let chunk_contents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();

    // Generate embeddings
    let embeddings = embedder
        .embed(&chunk_contents)
        .with_context(|| format!("Failed to generate embeddings for {:?}", path))?;

    // Create indexed chunks
    let file_path_str = path.to_string_lossy().to_string();
    for (chunk, embedding) in chunks.iter().zip(embeddings.into_iter()) {
        all_chunks.push(IndexedChunk {
            id: uuid::Uuid::new_v4().to_string(),
            content: chunk.content.clone(),
            file_path: file_path_str.clone(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            language: chunk.language.clone(),
            vector: embedding,
            mtime,
        });
    }

    total_chunks += chunks.len();

    // Insert in batches
    if all_chunks.len() >= batch_size * 10 {
        storage.insert_chunks(all_chunks.clone()).await?;
        all_chunks.clear();
    }
}
```

**Processing Steps:**
1. File discovery via `Walker::collect_files()` (sequential)
2. Sequential iteration over files to index
3. **Per-file I/O:** `fs::read_to_string(path)` - **BLOCKING**
4. **Per-file computation:** `chunker.chunk_file(path, &content)` - **CPU-bound**
5. **Per-batch I/O:** `embedder.embed(&chunk_contents)` - **Network/async**
6. **Per-batch I/O:** `storage.insert_chunks()` - **async**
7. **Build secondary index:** BM25 rebuild (lines 120-137)

**Bottlenecks Identified:**
- File I/O is sequential and blocking
- Chunking is CPU-bound and sequential
- Embedding calls are batched but still sequential per file batch
- No parallelism across multiple files

---

### 1.2 Watch Mode - `src/watcher/handler.rs`

**File Path:** `/home/nolood/general/coderag/src/watcher/handler.rs`

**Current Sequential Processing (Lines 95-140):**
```rust
pub async fn process_changes(&mut self, changes: Vec<FileChange>) -> Result<ProcessingStats> {
    let mut stats = ProcessingStats::default();

    for change in changes {
        match self.process_single(&change).await {
            Ok(single_stats) => {
                stats.merge(&single_stats);
            }
            Err(e) => {
                error!("Failed to process {:?}: {}", change.path, e);
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}
```

**Per-File Processing (Lines 145-180):**
```rust
async fn process_single(&mut self, change: &FileChange) -> Result<ProcessingStats> {
    // Pattern matching on change type
    match change.change_type {
        ChangeType::Created => {
            stats.chunks_created = self.index_file(&change.path).await?;
            stats.files_added = 1;
        }
        ChangeType::Modified => {
            stats.chunks_removed = self.delete_file_chunks(&change.path).await?;
            stats.chunks_created = self.index_file(&change.path).await?;
            stats.files_modified = 1;
        }
        ChangeType::Deleted => {
            stats.chunks_removed = self.delete_file_chunks(&change.path).await?;
            stats.files_deleted = 1;
        }
    }

    Ok(stats)
}
```

**Index File Operation (Lines 190-245):**
```rust
async fn index_file(&mut self, path: &PathBuf) -> Result<usize> {
    // Read file content - BLOCKING I/O
    let content = match fs::read_to_string(path) { ... }

    // Get mtime - BLOCKING I/O
    let mtime = get_file_mtime(path).unwrap_or(0);

    // Chunk the file - CPU-bound
    let chunks = self.chunker.chunk_file(path, &content);

    // Generate embeddings - async
    let embeddings = self.embedder.embed(&chunk_contents).await?;

    // Insert chunks - async
    self.storage.insert_chunks(indexed_chunks).await?;

    Ok(chunk_count)
}
```

**Bottlenecks Identified:**
- Changes are processed sequentially in a loop
- File I/O is blocking (should use `tokio::fs`)
- Each file is processed completely before next file starts
- No concurrent embedding generation across multiple files
- Mutable `self` prevents parallel access

---

### 1.3 Watch Event Loop - `src/watcher/mod.rs`

**File Path:** `/home/nolood/general/coderag/src/watcher/mod.rs`

**Event Loop (Lines 118-175):**
```rust
loop {
    tokio::select! {
        _ = &mut shutdown_rx => {
            info!("Shutdown signal received, stopping watcher");
            break;
        }

        Some(events) = rx.recv() => {
            let changes = self.convert_events(events);

            if !changes.is_empty() {
                info!("Processing {} file changes", changes.len());

                match handler.process_changes(changes).await {
                    Ok(stats) => {
                        total_stats.merge(&stats);
                        Self::print_stats(&stats);
                    }
                    Err(e) => {
                        error!("Failed to process changes: {}", e);
                        total_stats.errors += 1;
                    }
                }
            }
        }
    }
}
```

**Note:** The debouncer already batches events, but processing within a batch is sequential.

---

## 2. Existing Concurrency Analysis

### 2.1 Current Async Patterns

**Async traits used:**
- `Storage::insert_chunks()` - async
- `Storage::delete_by_file()` - async
- `EmbeddingGenerator::embed()` - async (network calls)
- `ChangeHandler::process_changes()` - async (but sequential loop)

**Mutex/Lock Usage:**
- `Arc<Storage>` - shared, async-safe
- `Arc<EmbeddingGenerator>` - shared, async-safe
- `RwLock<Bm25Index>` - used for BM25 search operations

**Thread Safety:**
- Components are designed to be thread-safe (`Send + Sync`)
- No global state corruption risks identified

### 2.2 Lack of Parallel Patterns

Currently **NO parallel processing:**
- No `rayon` dependency in `Cargo.toml`
- File loops are purely sequential
- No `tokio::task::spawn` for concurrent file processing
- Embeddings generated in batches per file, not across files

---

## 3. Rayon Integration Points

### 3.1 Parallel File Processing with Rayon

**Use Case:** Process multiple files in parallel using `par_iter()`.

**Code Example Pattern:**
```rust
use rayon::prelude::*;

// Current sequential:
for path in files_to_index.iter() { ... }

// Rayon parallel:
files_to_index.par_iter()
    .map(|path| process_file(path))
    .collect()
```

**Advantages:**
- Data-level parallelism across multiple files
- Automatic thread pool management
- Zero-copy data sharing via thread-safe references
- Natural batching of work

### 3.2 Parallel Chunking

**Use Case:** Chunk multiple files in parallel after I/O.

```rust
let all_chunks: Vec<IndexedChunk> = files_to_process.par_iter()
    .flat_map(|(path, content, mtime)| {
        let chunks = chunker.chunk_file(path, &content);
        chunks.par_iter()
            .map(|chunk| IndexedChunk { ... })
            .collect::<Vec<_>>()
    })
    .collect();
```

**Advantages:**
- CPU-bound chunking parallelized
- Chunker can be cloned/shared across threads
- Results naturally batched for storage

### 3.3 Embedding Batch Optimization

**Current bottleneck:** Embeddings generated serially per file.

**Rayon enhancement:**
```rust
// Collect ALL chunks from all files first
let all_chunks_content: Vec<String> = all_chunks.iter()
    .map(|c| c.content.clone())
    .collect();

// Generate embeddings in one large batch
let embeddings = embedder.embed(&all_chunks_content).await?;

// Assign embeddings back in parallel
let result: Vec<IndexedChunk> = all_chunks.par_iter()
    .zip(embeddings.par_iter())
    .map(|(chunk, embedding)| IndexedChunk { vector: embedding.clone(), ..chunk.clone() })
    .collect();
```

**Advantage:** Single embedding call for all chunks vs. per-file batching.

---

## 4. Tokio + Rayon Integration Patterns

### 4.1 Blocking Operations with `spawn_blocking`

**Problem:** File I/O is blocking and should not run on Tokio async threads.

**Solution Pattern:**
```rust
use tokio::task;
use rayon::prelude::*;

// Spawn blocking file I/O on dedicated thread pool
let file_contents = task::spawn_blocking(move || {
    files_to_index.par_iter()
        .map(|path| {
            match fs::read_to_string(path) {
                Ok(content) => Some((path.clone(), content)),
                Err(e) => {
                    debug!("Error reading {:?}: {}", path, e);
                    None
                }
            }
        })
        .filter_map(|x| x)
        .collect::<Vec<_>>()
}).await?;
```

**Key Points:**
- File I/O runs on Tokio's blocking thread pool
- Rayon parallelism runs within each blocking thread
- No interference with async runtime
- Tokio manages thread pool size automatically

### 4.2 Async + Parallel Chunking Pattern

**Pattern:**
```rust
// Phase 1: Read files (blocking, parallel)
let file_contents = task::spawn_blocking(move || {
    files.par_iter()
        .filter_map(|path| {
            fs::read_to_string(path).ok()
                .map(|content| (path.clone(), content))
        })
        .collect::<Vec<_>>()
}).await?;

// Phase 2: Chunk files (CPU-bound, parallel)
let chunked = task::spawn_blocking(move || {
    file_contents.par_iter()
        .flat_map(|(path, content)| {
            chunker.chunk_file(path, content).into_par_iter()
        })
        .collect::<Vec<_>>()
}).await?;

// Phase 3: Generate embeddings (async, all at once)
let embeddings = embedder.embed(&all_content).await?;

// Phase 4: Store chunks (async)
storage.insert_chunks(final_chunks).await?;
```

### 4.3 Concurrent File Processing in Watch Mode

**Problem:** `ChangeHandler` is not `Send` due to `&mut self`.

**Solution Pattern:**
```rust
// Make components cloneable or wrap in Arc
pub struct ChangeHandler {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    chunker: Arc<Chunker>,  // Make Arc for sharing
}

// Process changes concurrently
pub async fn process_changes_concurrent(&self, changes: Vec<FileChange>) -> Result<ProcessingStats> {
    let futures = changes.into_iter()
        .map(|change| {
            let handler = self.clone();
            tokio::spawn(async move {
                handler.process_single(&change).await
            })
        })
        .collect::<Vec<_>>();

    let mut stats = ProcessingStats::default();
    for future in futures {
        match future.await? {
            Ok(single_stats) => stats.merge(&single_stats),
            Err(e) => {
                error!("Failed to process change: {}", e);
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}
```

---

## 5. Performance Considerations

### 5.1 Thread Pool Configuration

**Rayon Thread Pool:**
- Default: Number of logical CPU cores
- Configuration: `rayon::ThreadPoolBuilder`
- Use case: CPU-bound work (chunking, hashing)

```rust
use rayon::ThreadPoolBuilder;

let pool = ThreadPoolBuilder::new()
    .num_threads(num_cpus::get())
    .build_global()
    .unwrap();
```

**Tokio Thread Pool:**
- Worker threads: Number of cores for async runtime
- Blocking threads: Scaled automatically (default 512 max)
- Use case: Async I/O and blocking operations

```rust
// Spawn blocking is automatic, no config needed
// But can set max_blocking_threads on RuntimeBuilder
let rt = tokio::runtime::Builder::new_multi_thread()
    .max_blocking_threads(1024)
    .build()?;
```

### 5.2 Memory Considerations

**Current bottleneck:** `all_chunks` vector grows unbounded until batch insert.

```rust
let mut all_chunks: Vec<IndexedChunk> = Vec::new();

for path in files_to_index {
    // ... processing ...
    all_chunks.push(...);

    // Insert every (batch_size * 10) chunks
    if all_chunks.len() >= batch_size * 10 {
        storage.insert_chunks(all_chunks.clone()).await?;
        all_chunks.clear();
    }
}
```

**Optimization:**
- Use smaller batches with parallel processing
- Process results as streams rather than collecting all
- Implement backpressure: don't process next file until batch inserted

### 5.3 I/O Saturation

**Risk:** Too many parallel file reads could saturate disk I/O.

**Mitigation:**
- Limit parallelism with `rayon::scope`
- Use `task::spawn_blocking` which automatically limits
- Implement bounded channels for backpressure
- Monitor actual performance (may not need full parallelism)

### 5.4 Embedding Generation Bottleneck

Current: Embeddings are the most expensive operation (network + model).

**Pattern:**
```rust
// Collect ALL content first (parallel)
let all_content: Vec<String> = chunked_results.par_iter()
    .map(|chunk| chunk.content.clone())
    .collect();

// Single async call (most efficient)
let embeddings = embedder.embed(&all_content).await?;

// Assign back in parallel
```

**Benefit:** Amortize network latency across maximum chunks.

---

## 6. Thread Pool Configuration Recommendations

### 6.1 Rayon Configuration

**For CodeRAG (CPU-bound chunking):**
```rust
// In Cargo.toml
[dependencies]
rayon = "1.7"

// In code
use rayon::prelude::*;

// Use default pool (# of cores)
// Custom if needed:
let pool = rayon::ThreadPoolBuilder::new()
    .num_threads(num_cpus::get())
    .stack_size(2 * 1024 * 1024)  // 2MB per thread
    .build_global();
```

### 6.2 Tokio Configuration

**For CodeRAG (mixed async/blocking):**
```rust
// Default tokio::main is fine for most cases
#[tokio::main]
async fn main() -> Result<()> { ... }

// Custom if needed:
let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(num_cpus::get())
    .max_blocking_threads(512)  // Default is good
    .enable_all()
    .build()?;
```

### 6.3 Recommended Settings

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Rayon threads | `num_cpus::get()` | Match CPU cores for parallel chunking |
| Tokio workers | `num_cpus::get()` | Standard async runtime config |
| Tokio blocking | 512 (default) | Adequate for file I/O operations |
| Batch size | 128-256 chunks | Balance memory vs. embedding efficiency |
| File read ahead | rayon scope limit | Prevent disk I/O saturation |

---

## 7. Integration Roadmap

### Phase 1: File I/O Parallelization (Priority: HIGH)
- [ ] Add `tokio::fs` for async file reading
- [ ] Use `task::spawn_blocking` for parallel file discovery
- [ ] Migrate `fs::read_to_string` to `tokio::fs::read_to_string`

### Phase 2: Chunking Parallelization (Priority: HIGH)
- [ ] Add Rayon dependency
- [ ] Use `par_iter()` for file chunking
- [ ] Benchmark: sequential vs. parallel chunking

### Phase 3: Embedding Optimization (Priority: MEDIUM)
- [ ] Collect all chunks before embedding
- [ ] Single batch embedding call
- [ ] Async embed + parallel chunk assignment

### Phase 4: Watch Mode Concurrency (Priority: MEDIUM)
- [ ] Make `ChangeHandler` send-safe (Arc components)
- [ ] Use `tokio::spawn` for concurrent file processing
- [ ] Add concurrency limits with semaphore

### Phase 5: Monitoring & Tuning (Priority: LOW)
- [ ] Add performance metrics
- [ ] Thread pool utilization tracking
- [ ] Benchmark on various hardware

---

## 8. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|-----------|
| Disk I/O saturation | Medium | High | Limit parallelism, use backpressure |
| Memory spike | Low | Medium | Smaller batches, streaming results |
| Complexity increase | High | Medium | Phased implementation, tests |
| Embedding API limits | Low | High | Batch limiting, rate control |
| Thread contention | Low | Low | Proper scoping, pool sizing |

---

## 9. References & Code Snippets

### Rayon Key Methods
```rust
par_iter()          // Convert iterator to parallel
map()               // Parallel map transformation
flat_map()          // Parallel flat map
filter_map()        // Parallel filter + map
collect()           // Gather results
for_each()          // Parallel side effects
```

### Tokio Key Functions
```rust
spawn()             // Spawn async task
spawn_blocking()    // Spawn blocking task
task::JoinSet       // Manage multiple spawned tasks
select!             // Async multiplexing
```

### Pattern: Tokio + Rayon
```rust
tokio::task::spawn_blocking(|| {
    data.par_iter()
        .map(process)
        .collect()
}).await
```

---

## 10. Conclusion

CodeRAG has clear opportunities for parallelization:

1. **File I/O** - Convert to async with `tokio::fs` and use `spawn_blocking`
2. **Chunking** - Parallelize with Rayon's `par_iter()`
3. **Embedding** - Batch optimization across all files
4. **Watch Mode** - Concurrent change processing with task spawning

**Expected improvements:**
- 3-5x faster batch indexing (multi-file parallelism)
- 2-3x faster watch mode (concurrent file processing)
- Better responsiveness during large file changes
- Minimal complexity increase with phased approach

**Next Steps:** Implement Phase 1 (File I/O) and Phase 2 (Chunking) for maximum ROI.
