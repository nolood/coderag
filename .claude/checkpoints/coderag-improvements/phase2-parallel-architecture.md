# Phase 2: Parallel File Indexing Architecture Design

**Date:** 2025-12-06
**Author:** Backend Architect
**Focus:** Parallel processing pipeline architecture for file indexing system

---

## Executive Summary

This document outlines a comprehensive parallel processing architecture for CodeRAG's file indexing system. The design integrates Tokio async runtime with Rayon for CPU-bound parallelism while maintaining thread-safe storage operations and preserving error reporting capabilities.

---

## 1. System Architecture Overview

### 1.1 High-Level Pipeline Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                     PARALLEL INDEXING PIPELINE                │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Stage 1: File Discovery (Sequential)                        │
│  ├─ Walker::collect_files()                                 │
│  └─ Output: Vec<PathBuf>                                    │
│                           ▼                                  │
│  ┌──────────────────────────────────────────────────┐      │
│  │         PARALLEL PROCESSING BOUNDARY              │      │
│  └──────────────────────────────────────────────────┘      │
│                           ▼                                  │
│  Stage 2: Parallel File I/O (spawn_blocking + Rayon)        │
│  ├─ Tokio blocking thread pool                              │
│  ├─ Rayon par_iter() for multiple files                     │
│  └─ Output: Vec<(PathBuf, String, i64)>                     │
│                           ▼                                  │
│  Stage 3: Parallel Chunking (spawn_blocking + Rayon)        │
│  ├─ CPU-bound operation                                     │
│  ├─ Rayon par_flat_map() across files                       │
│  └─ Output: Vec<RawChunk>                                   │
│                           ▼                                  │
│  Stage 4: Batch Embedding (Async)                           │
│  ├─ Single batch call to embedding service                  │
│  ├─ Async network operation                                 │
│  └─ Output: Vec<Vec<f32>>                                   │
│                           ▼                                  │
│  Stage 5: Parallel Assembly (spawn_blocking + Rayon)        │
│  ├─ Zip chunks with embeddings                              │
│  ├─ Create IndexedChunk structures                          │
│  └─ Output: Vec<IndexedChunk>                               │
│                           ▼                                  │
│  Stage 6: Storage Operations (Async)                        │
│  ├─ Batch insert with backpressure                          │
│  ├─ Thread-safe storage access                              │
│  └─ BM25 index rebuild                                      │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

### 1.2 Architectural Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Concurrency Model** | Hybrid Tokio + Rayon | Tokio for async I/O, Rayon for CPU-bound parallelism |
| **File I/O Strategy** | Parallel blocking reads | Prevent blocking async runtime, leverage OS file cache |
| **Chunking Strategy** | Parallel CPU-bound | Maximize CPU utilization for text processing |
| **Embedding Strategy** | Single batch async | Minimize network latency, maximize throughput |
| **Storage Strategy** | Batched async inserts | Reduce database transaction overhead |
| **Error Handling** | Per-file collection | Continue processing on errors, report all failures |
| **Memory Management** | Streaming with backpressure | Prevent OOM on large codebases |

---

## 2. Detailed Component Architecture

### 2.1 Parallel File Processing Pipeline

```rust
// Core pipeline data structures
pub struct FileContent {
    path: PathBuf,
    content: String,
    mtime: i64,
}

pub struct RawChunk {
    content: String,
    file_path: String,
    start_line: usize,
    end_line: usize,
    language: Option<String>,
    mtime: i64,
}

pub struct ProcessingResult {
    successful: Vec<IndexedChunk>,
    errors: Vec<FileError>,
}

pub struct FileError {
    path: PathBuf,
    error: String,
    stage: ProcessingStage,
}

#[derive(Debug)]
pub enum ProcessingStage {
    FileRead,
    Chunking,
    Embedding,
    Storage,
}
```

### 2.2 Thread Pool Configuration

```rust
pub struct ParallelConfig {
    // Rayon configuration
    rayon_threads: usize,        // Default: num_cpus::get()
    rayon_stack_size: usize,     // Default: 2MB

    // Tokio configuration
    tokio_workers: usize,         // Default: num_cpus::get()
    tokio_blocking_threads: usize, // Default: 512

    // Processing configuration
    file_batch_size: usize,       // Files to process in parallel (Default: 100)
    chunk_batch_size: usize,      // Chunks per embedding batch (Default: 256)
    storage_batch_size: usize,    // Chunks per storage batch (Default: 1000)

    // Backpressure configuration
    max_pending_chunks: usize,    // Max chunks in memory (Default: 10000)
    max_concurrent_files: usize,  // Max files processing simultaneously (Default: 50)
}

impl Default for ParallelConfig {
    fn default() -> Self {
        let cpu_count = num_cpus::get();
        Self {
            rayon_threads: cpu_count,
            rayon_stack_size: 2 * 1024 * 1024, // 2MB
            tokio_workers: cpu_count,
            tokio_blocking_threads: 512,
            file_batch_size: 100,
            chunk_batch_size: 256,
            storage_batch_size: 1000,
            max_pending_chunks: 10000,
            max_concurrent_files: 50,
        }
    }
}
```

### 2.3 Tokio + Rayon Integration Pattern

```rust
pub struct ParallelIndexer {
    config: ParallelConfig,
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    chunker: Arc<Chunker>,
    semaphore: Arc<Semaphore>, // Backpressure control
}

impl ParallelIndexer {
    pub async fn index_files(&self, files: Vec<PathBuf>) -> Result<ProcessingResult> {
        // Initialize Rayon thread pool
        self.init_rayon_pool()?;

        // Stage 1: Filter files needing indexing (sequential)
        let files_to_index = self.filter_modified_files(files).await?;

        // Stage 2: Parallel file reading with bounded concurrency
        let file_contents = self.read_files_parallel(files_to_index).await?;

        // Stage 3: Parallel chunking
        let raw_chunks = self.chunk_files_parallel(file_contents).await?;

        // Stage 4: Batch embedding generation
        let embeddings = self.generate_embeddings_batch(raw_chunks).await?;

        // Stage 5: Parallel assembly of indexed chunks
        let indexed_chunks = self.assemble_chunks_parallel(raw_chunks, embeddings).await?;

        // Stage 6: Storage with backpressure
        self.store_chunks_with_backpressure(indexed_chunks).await?;

        Ok(ProcessingResult { ... })
    }
}
```

---

## 3. Stage-Specific Implementations

### 3.1 Stage 2: Parallel File I/O

```rust
async fn read_files_parallel(&self, files: Vec<PathBuf>) -> Result<Vec<FileContent>> {
    // Use spawn_blocking to avoid blocking async runtime
    let chunk_size = self.config.file_batch_size;
    let mut all_contents = Vec::new();

    // Process files in batches to control memory usage
    for batch in files.chunks(chunk_size) {
        let batch = batch.to_vec();
        let contents = tokio::task::spawn_blocking(move || {
            batch.par_iter()
                .filter_map(|path| {
                    match fs::read_to_string(path) {
                        Ok(content) => {
                            let mtime = get_file_mtime(path).unwrap_or(0);
                            Some(FileContent {
                                path: path.clone(),
                                content,
                                mtime,
                            })
                        }
                        Err(e) => {
                            // Collect error for reporting
                            None // Continue processing other files
                        }
                    }
                })
                .collect::<Vec<_>>()
        }).await?;

        all_contents.extend(contents);
    }

    Ok(all_contents)
}
```

### 3.2 Stage 3: Parallel Chunking

```rust
async fn chunk_files_parallel(&self, files: Vec<FileContent>) -> Result<Vec<RawChunk>> {
    let chunker = self.chunker.clone();

    tokio::task::spawn_blocking(move || {
        files.par_iter()
            .flat_map(|file| {
                let chunks = chunker.chunk_file(&file.path, &file.content);
                chunks.into_par_iter()
                    .map(|chunk| RawChunk {
                        content: chunk.content,
                        file_path: file.path.to_string_lossy().to_string(),
                        start_line: chunk.start_line,
                        end_line: chunk.end_line,
                        language: chunk.language,
                        mtime: file.mtime,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }).await?
}
```

### 3.3 Stage 4: Batch Embedding Generation

```rust
async fn generate_embeddings_batch(&self, chunks: Vec<RawChunk>) -> Result<Vec<Vec<f32>>> {
    // Collect all chunk content for single batch processing
    let contents: Vec<String> = chunks.iter()
        .map(|c| c.content.clone())
        .collect();

    // Process in batches if exceeding max batch size
    let mut all_embeddings = Vec::new();
    for batch in contents.chunks(self.config.chunk_batch_size) {
        let embeddings = self.embedder.embed(batch).await?;
        all_embeddings.extend(embeddings);
    }

    Ok(all_embeddings)
}
```

### 3.4 Stage 5: Parallel Assembly

```rust
async fn assemble_chunks_parallel(
    &self,
    chunks: Vec<RawChunk>,
    embeddings: Vec<Vec<f32>>
) -> Result<Vec<IndexedChunk>> {
    tokio::task::spawn_blocking(move || {
        chunks.into_par_iter()
            .zip(embeddings.into_par_iter())
            .map(|(chunk, embedding)| IndexedChunk {
                id: uuid::Uuid::new_v4().to_string(),
                content: chunk.content,
                file_path: chunk.file_path,
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                language: chunk.language,
                vector: embedding,
                mtime: chunk.mtime,
            })
            .collect()
    }).await?
}
```

### 3.5 Stage 6: Storage with Backpressure

```rust
async fn store_chunks_with_backpressure(&self, chunks: Vec<IndexedChunk>) -> Result<()> {
    // Process in batches with semaphore-based backpressure
    for batch in chunks.chunks(self.config.storage_batch_size) {
        // Acquire permit to ensure we don't overwhelm storage
        let _permit = self.semaphore.acquire().await?;

        let batch = batch.to_vec();
        let storage = self.storage.clone();

        // Spawn storage operation
        tokio::spawn(async move {
            if let Err(e) = storage.insert_chunks(batch).await {
                error!("Failed to insert batch: {}", e);
            }
            // Permit released automatically when dropped
        });
    }

    Ok(())
}
```

---

## 4. Watch Mode Parallel Architecture

### 4.1 Concurrent Change Processing

```rust
pub struct ParallelChangeHandler {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    chunker: Arc<Chunker>,
    semaphore: Arc<Semaphore>, // Limit concurrent file processing
}

impl ParallelChangeHandler {
    pub async fn process_changes_concurrent(
        &self,
        changes: Vec<FileChange>
    ) -> Result<ProcessingStats> {
        let mut handles = Vec::new();

        for change in changes {
            let handler = self.clone();
            let permit = self.semaphore.clone().acquire_owned().await?;

            let handle = tokio::spawn(async move {
                let _permit = permit; // Hold permit for duration
                handler.process_single_change(change).await
            });

            handles.push(handle);
        }

        // Collect results
        let mut total_stats = ProcessingStats::default();
        for handle in handles {
            match handle.await? {
                Ok(stats) => total_stats.merge(&stats),
                Err(e) => {
                    error!("Change processing failed: {}", e);
                    total_stats.errors += 1;
                }
            }
        }

        Ok(total_stats)
    }
}
```

### 4.2 Event Batching Strategy

```rust
pub struct BatchedEventProcessor {
    batch_timeout: Duration,      // Max time to wait for batch (100ms)
    batch_size: usize,            // Max events per batch (50)
    handler: Arc<ParallelChangeHandler>,
}

impl BatchedEventProcessor {
    pub async fn process_event_stream(&self, mut rx: Receiver<FileChange>) {
        let mut batch = Vec::new();
        let mut batch_timer = tokio::time::interval(self.batch_timeout);

        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    batch.push(event);
                    if batch.len() >= self.batch_size {
                        self.flush_batch(&mut batch).await;
                    }
                }
                _ = batch_timer.tick() => {
                    if !batch.is_empty() {
                        self.flush_batch(&mut batch).await;
                    }
                }
            }
        }
    }

    async fn flush_batch(&self, batch: &mut Vec<FileChange>) {
        if batch.is_empty() { return; }

        let changes = std::mem::take(batch);
        if let Err(e) = self.handler.process_changes_concurrent(changes).await {
            error!("Batch processing failed: {}", e);
        }
    }
}
```

---

## 5. Error Handling Architecture

### 5.1 Error Collection Pattern

```rust
pub struct ErrorCollector {
    errors: Arc<Mutex<Vec<FileError>>>,
    max_errors: usize, // Stop processing if exceeded
}

impl ErrorCollector {
    pub fn record(&self, path: PathBuf, error: anyhow::Error, stage: ProcessingStage) {
        let mut errors = self.errors.lock().unwrap();
        errors.push(FileError {
            path,
            error: error.to_string(),
            stage,
        });
    }

    pub fn should_continue(&self) -> bool {
        self.errors.lock().unwrap().len() < self.max_errors
    }

    pub fn get_report(&self) -> ErrorReport {
        let errors = self.errors.lock().unwrap();
        ErrorReport::from_errors(&errors)
    }
}
```

### 5.2 Error Reporting Structure

```rust
pub struct ErrorReport {
    total_errors: usize,
    by_stage: HashMap<ProcessingStage, Vec<FileError>>,
    summary: String,
}

impl ErrorReport {
    pub fn print_summary(&self) {
        println!("⚠️  Processing completed with {} errors", self.total_errors);

        for (stage, errors) in &self.by_stage {
            println!("  {:?}: {} errors", stage, errors.len());
            for error in errors.iter().take(5) {
                println!("    - {}: {}", error.path.display(), error.error);
            }
        }
    }
}
```

---

## 6. Memory Management Strategy

### 6.1 Streaming Architecture

```rust
pub struct StreamingIndexer {
    chunk_stream: Arc<Mutex<VecDeque<RawChunk>>>,
    max_buffered: usize,
    producer_handle: Option<JoinHandle<()>>,
    consumer_handle: Option<JoinHandle<()>>,
}

impl StreamingIndexer {
    pub async fn start_streaming(&mut self, files: Vec<PathBuf>) {
        // Producer task - generates chunks
        let producer = self.spawn_producer(files);

        // Consumer task - processes chunks
        let consumer = self.spawn_consumer();

        self.producer_handle = Some(producer);
        self.consumer_handle = Some(consumer);
    }

    fn spawn_producer(&self, files: Vec<PathBuf>) -> JoinHandle<()> {
        let stream = self.chunk_stream.clone();
        let max_buffered = self.max_buffered;

        tokio::spawn(async move {
            for batch in files.chunks(10) {
                // Wait if buffer is full (backpressure)
                while stream.lock().unwrap().len() >= max_buffered {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }

                // Process batch and add to stream
                let chunks = process_file_batch(batch).await;
                stream.lock().unwrap().extend(chunks);
            }
        })
    }
}
```

### 6.2 Memory Limits

```rust
pub struct MemoryLimits {
    max_file_size: usize,        // Skip files larger than this (100MB)
    max_chunk_size: usize,       // Max size per chunk (1MB)
    max_total_memory: usize,     // Max memory for all buffers (1GB)
    current_usage: AtomicUsize,  // Track current memory usage
}

impl MemoryLimits {
    pub fn can_allocate(&self, size: usize) -> bool {
        let current = self.current_usage.load(Ordering::Relaxed);
        current + size <= self.max_total_memory
    }

    pub fn allocate(&self, size: usize) -> Option<MemoryGuard> {
        if self.can_allocate(size) {
            self.current_usage.fetch_add(size, Ordering::Relaxed);
            Some(MemoryGuard { limiter: self, size })
        } else {
            None
        }
    }
}
```

---

## 7. Progress Reporting Integration

### 7.1 Multi-Stage Progress

```rust
pub struct ParallelProgressReporter {
    multi_progress: MultiProgress,
    file_progress: ProgressBar,
    chunk_progress: ProgressBar,
    embedding_progress: ProgressBar,
    storage_progress: ProgressBar,
}

impl ParallelProgressReporter {
    pub fn new(total_files: usize) -> Self {
        let multi = MultiProgress::new();

        let file_pb = multi.add(ProgressBar::new(total_files as u64));
        file_pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed}] Files: {bar:40} {pos}/{len}")
            .unwrap());

        // Similar for other progress bars...

        Self {
            multi_progress: multi,
            file_progress: file_pb,
            // ...
        }
    }

    pub fn update_file_progress(&self, completed: usize) {
        self.file_progress.set_position(completed as u64);
    }
}
```

### 7.2 Thread-Safe Progress Updates

```rust
pub struct ThreadSafeProgress {
    progress: Arc<Mutex<ProgressBar>>,
}

impl ThreadSafeProgress {
    pub fn increment(&self) {
        self.progress.lock().unwrap().inc(1);
    }

    pub fn set_message(&self, msg: String) {
        self.progress.lock().unwrap().set_message(msg);
    }
}
```

---

## 8. Performance Expectations

### 8.1 Theoretical Improvements

| Operation | Sequential (Current) | Parallel (Proposed) | Expected Speedup |
|-----------|---------------------|---------------------|------------------|
| **File Reading** | O(n) | O(n/p) | 4-8x on SSD |
| **Chunking** | O(n*m) | O(n*m/p) | Near-linear with CPU cores |
| **Embedding** | O(n) batches | O(1) mega-batch | 3-5x fewer API calls |
| **Storage** | O(n) transactions | O(n/b) transactions | 2-3x fewer DB ops |
| **Overall** | Linear | Parallel pipeline | **3-5x faster** |

Where:
- n = number of files
- m = chunks per file
- p = parallelism level
- b = batch size

### 8.2 Real-World Expectations

```
Small Codebase (100 files, 1000 chunks):
- Current: ~10 seconds
- Parallel: ~3 seconds
- Speedup: 3.3x

Medium Codebase (1000 files, 10000 chunks):
- Current: ~100 seconds
- Parallel: ~25 seconds
- Speedup: 4x

Large Codebase (10000 files, 100000 chunks):
- Current: ~1000 seconds (16.7 min)
- Parallel: ~200 seconds (3.3 min)
- Speedup: 5x
```

### 8.3 Bottleneck Analysis

```
Pipeline Bottlenecks (sorted by impact):
1. Embedding Generation: 40-50% of total time (network-bound)
2. File I/O: 20-30% of total time (disk-bound)
3. Chunking: 15-20% of total time (CPU-bound)
4. Storage: 10-15% of total time (database-bound)
5. Overhead: 5-10% (coordination, memory management)
```

---

## 9. Testing Strategy

### 9.1 Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_file_reading() {
        let indexer = ParallelIndexer::new_test();
        let files = generate_test_files(100);

        let start = Instant::now();
        let results = indexer.read_files_parallel(files).await.unwrap();
        let duration = start.elapsed();

        assert_eq!(results.len(), 100);
        assert!(duration.as_secs() < 2); // Should be fast
    }

    #[tokio::test]
    async fn test_error_collection() {
        let indexer = ParallelIndexer::new_test();
        let files = vec![
            PathBuf::from("/nonexistent1.rs"),
            PathBuf::from("/nonexistent2.rs"),
            PathBuf::from("/valid.rs"),
        ];

        let result = indexer.index_files(files).await.unwrap();
        assert_eq!(result.errors.len(), 2);
        assert_eq!(result.successful.len(), 1);
    }
}
```

### 9.2 Integration Testing

```rust
#[tokio::test]
async fn test_full_pipeline() {
    let temp_dir = tempdir().unwrap();
    create_test_project(&temp_dir, 50); // 50 files

    let config = Config::default();
    let indexer = ParallelIndexer::new(config);

    let start = Instant::now();
    let result = indexer.index_directory(temp_dir.path()).await.unwrap();
    let duration = start.elapsed();

    // Verify results
    assert!(result.successful.len() > 0);
    assert_eq!(result.errors.len(), 0);

    // Verify performance
    let sequential_estimate = 50 * 100; // ms per file
    assert!(duration.as_millis() < sequential_estimate / 3);
}
```

### 9.3 Benchmark Suite

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_parallel_indexing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("parallel_100_files", |b| {
        b.to_async(&rt).iter(|| async {
            let indexer = create_test_indexer();
            let files = generate_files(100);
            indexer.index_files(black_box(files)).await
        });
    });

    c.bench_function("sequential_100_files", |b| {
        b.to_async(&rt).iter(|| async {
            let indexer = create_sequential_indexer();
            let files = generate_files(100);
            indexer.index_files(black_box(files)).await
        });
    });
}

criterion_group!(benches, benchmark_parallel_indexing);
criterion_main!(benches);
```

### 9.4 Stress Testing

```rust
#[tokio::test]
async fn stress_test_memory_limits() {
    let config = ParallelConfig {
        max_pending_chunks: 100,
        max_concurrent_files: 5,
        ..Default::default()
    };

    let indexer = ParallelIndexer::new(config);
    let files = generate_large_files(1000); // 1000 large files

    // Should not OOM
    let result = indexer.index_files(files).await;
    assert!(result.is_ok());

    // Verify memory was bounded
    let peak_memory = get_peak_memory_usage();
    assert!(peak_memory < 2 * 1024 * 1024 * 1024); // Less than 2GB
}
```

---

## 10. Implementation Roadmap

### Phase 1: Foundation (Week 1)
- [ ] Add Rayon dependency to Cargo.toml
- [ ] Create ParallelConfig structure
- [ ] Implement thread pool initialization
- [ ] Add error collection infrastructure

### Phase 2: File I/O Parallelization (Week 1)
- [ ] Implement parallel file reading with spawn_blocking
- [ ] Add file batching logic
- [ ] Create FileContent structure
- [ ] Add progress reporting for file I/O

### Phase 3: Chunking Parallelization (Week 2)
- [ ] Make Chunker thread-safe (Arc wrapper)
- [ ] Implement parallel chunking with Rayon
- [ ] Add RawChunk structure
- [ ] Integrate with error collector

### Phase 4: Embedding Optimization (Week 2)
- [ ] Implement mega-batch embedding strategy
- [ ] Add batch size limits
- [ ] Handle embedding errors gracefully
- [ ] Add retry logic for network failures

### Phase 5: Storage Integration (Week 3)
- [ ] Implement backpressure mechanism
- [ ] Add streaming storage inserts
- [ ] Integrate with semaphore limits
- [ ] Add transaction batching

### Phase 6: Watch Mode (Week 3)
- [ ] Convert ChangeHandler to Arc-based
- [ ] Implement concurrent change processing
- [ ] Add event batching
- [ ] Test with rapid file changes

### Phase 7: Testing & Optimization (Week 4)
- [ ] Comprehensive unit tests
- [ ] Integration test suite
- [ ] Benchmark comparisons
- [ ] Performance profiling
- [ ] Memory leak detection

### Phase 8: Documentation & Release (Week 4)
- [ ] API documentation
- [ ] Configuration guide
- [ ] Performance tuning guide
- [ ] Migration guide from sequential

---

## 11. Trade-offs and Considerations

### 11.1 Architectural Trade-offs

| Aspect | Benefit | Trade-off | Mitigation |
|--------|---------|-----------|------------|
| **Complexity** | 3-5x performance gain | Harder to debug | Comprehensive logging, error collection |
| **Memory Usage** | Parallel processing | Higher peak memory | Backpressure, streaming, bounded buffers |
| **CPU Usage** | Better utilization | Higher peak CPU | Configurable thread pools |
| **Error Handling** | Continue on failure | Complex error aggregation | Structured error reporting |
| **Testing** | Better coverage | More test scenarios | Automated test generation |

### 11.2 Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|---------|------------|
| **Disk I/O Saturation** | Medium | High | Bounded file parallelism, adaptive throttling |
| **Memory Exhaustion** | Low | Critical | Strict memory limits, backpressure |
| **Thread Pool Starvation** | Low | Medium | Separate pools for different operations |
| **Embedding API Rate Limits** | Medium | High | Exponential backoff, request queuing |
| **Database Lock Contention** | Low | Medium | Batch transactions, retry logic |

### 11.3 Configuration Recommendations

```yaml
# Recommended configuration for different scenarios

# Small projects (<1000 files)
small:
  rayon_threads: 4
  file_batch_size: 50
  chunk_batch_size: 128
  max_concurrent_files: 20

# Medium projects (1000-10000 files)
medium:
  rayon_threads: 8
  file_batch_size: 100
  chunk_batch_size: 256
  max_concurrent_files: 50

# Large projects (>10000 files)
large:
  rayon_threads: 16
  file_batch_size: 200
  chunk_batch_size: 512
  max_concurrent_files: 100
  max_pending_chunks: 20000
```

---

## 12. Monitoring and Observability

### 12.1 Metrics Collection

```rust
pub struct IndexingMetrics {
    files_processed: Counter,
    chunks_created: Counter,
    embeddings_generated: Counter,
    errors_encountered: Counter,

    file_read_duration: Histogram,
    chunking_duration: Histogram,
    embedding_duration: Histogram,
    storage_duration: Histogram,

    memory_usage: Gauge,
    thread_pool_utilization: Gauge,
}
```

### 12.2 Performance Dashboard

```
┌─────────────────────────────────────────────┐
│          Indexing Performance               │
├─────────────────────────────────────────────┤
│ Files/sec:        125.3                     │
│ Chunks/sec:       1,253.7                   │
│ Embeddings/sec:   892.1                     │
│                                             │
│ Stage Timings:                              │
│ ├─ File I/O:      12.3s (15%)              │
│ ├─ Chunking:      8.7s  (11%)              │
│ ├─ Embeddings:    45.2s (56%)              │
│ └─ Storage:       14.8s (18%)              │
│                                             │
│ Resource Usage:                             │
│ ├─ CPU:           78%                       │
│ ├─ Memory:        1.2GB / 2GB               │
│ └─ Disk I/O:      125 MB/s                 │
│                                             │
│ Errors:           3 / 1,234 files           │
└─────────────────────────────────────────────┘
```

---

## 13. Conclusion

This parallel architecture design provides a robust foundation for significantly improving CodeRAG's file indexing performance. Key benefits include:

1. **3-5x Performance Improvement**: Through parallel file processing and optimized batching
2. **Scalability**: Handles codebases from 100 to 100,000+ files efficiently
3. **Reliability**: Comprehensive error handling and recovery mechanisms
4. **Maintainability**: Clear separation of concerns and modular design
5. **Observability**: Built-in metrics and monitoring capabilities

The phased implementation approach ensures each component can be tested and validated independently before integration, minimizing risk and ensuring a smooth transition from the current sequential implementation.

---

## Appendix A: Code Structure Changes

```
src/
├── indexing/
│   ├── mod.rs
│   ├── parallel.rs         # New: Parallel indexer implementation
│   ├── config.rs          # New: Parallel configuration
│   ├── pipeline.rs        # New: Pipeline stages
│   ├── errors.rs          # New: Error collection
│   └── metrics.rs         # New: Performance metrics
├── commands/
│   └── index.rs           # Modified: Use parallel indexer
├── watcher/
│   ├── handler.rs         # Modified: Arc-based, concurrent
│   └── parallel.rs        # New: Parallel change processor
└── utils/
    ├── backpressure.rs    # New: Backpressure utilities
    └── streaming.rs       # New: Streaming utilities
```

## Appendix B: Dependency Changes

```toml
# Cargo.toml additions
[dependencies]
rayon = "1.7"
num_cpus = "1.16"
tokio = { version = "1.35", features = ["full"] }
crossbeam-channel = "0.5"  # For bounded channels
parking_lot = "0.12"        # For faster mutexes
dashmap = "5.5"            # For concurrent hashmaps

[dev-dependencies]
criterion = "0.5"          # For benchmarking
proptest = "1.4"          # For property testing
```