//! Parallel file indexing implementation using Rayon

use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, UNIX_EPOCH};
use tokio::sync::Semaphore;
use tracing::{error, info};

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::indexer::{AstChunker, Chunker, ChunkerStrategy, Walker};
use crate::storage::{IndexedChunk, Storage};

use super::errors::{ErrorCollector, ProcessingStage};
use super::pipeline::{FileContent, ProcessingResult, RawChunk};

/// Parallel indexer for processing files concurrently
pub struct ParallelIndexer {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    line_chunker: Option<Arc<Chunker>>,
    ast_chunker: Option<Arc<Mutex<AstChunker>>>,
    #[allow(dead_code)]
    walker: Arc<Walker>,
    config: Config,
    error_collector: ErrorCollector,
    semaphore: Arc<Semaphore>,
}

impl ParallelIndexer {
    /// Create a new parallel indexer
    pub async fn new(
        root: PathBuf,
        config: Config,
    ) -> Result<Self> {
        Self::with_storage_path(root, config, None).await
    }

    /// Create a new parallel indexer with a custom storage path.
    ///
    /// If `storage_path` is `None`, uses the default path from config.
    /// This allows the auto-index service to use global storage paths.
    pub async fn with_storage_path(
        root: PathBuf,
        config: Config,
        storage_path: Option<PathBuf>,
    ) -> Result<Self> {
        // Initialize Rayon thread pool if specified
        if let Some(threads) = config.indexer.parallel_threads {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build_global()
                .context("Failed to initialize Rayon thread pool")?;
        } else {
            // Use default thread pool (num_cpus)
            let threads = num_cpus::get();
            info!("Using {} threads for parallel processing", threads);
        }

        let db_path = storage_path.unwrap_or_else(|| config.db_path(&root));
        let storage = Arc::new(
            Storage::new(&db_path)
                .await
                .context("Failed to initialize storage")?
        );

        let embedder = Arc::new(
            EmbeddingGenerator::new(&config.embeddings)
                .context("Failed to initialize embedder")?
        );

        let walker = Arc::new(Walker::new(root.clone(), &config.indexer));

        // Initialize appropriate chunker based on strategy
        let (line_chunker, ast_chunker) = if config.indexer.chunker_strategy == ChunkerStrategy::Ast {
            (None, Some(Arc::new(Mutex::new(AstChunker::with_limits(
                config.indexer.min_chunk_tokens,
                config.indexer.max_chunk_tokens,
            )))))
        } else {
            (Some(Arc::new(Chunker::new(config.indexer.chunk_size))), None)
        };

        // Create semaphore for backpressure control
        let semaphore = Arc::new(Semaphore::new(config.indexer.max_concurrent_files));

        // Error collector with reasonable limit
        let error_collector = ErrorCollector::new(1000);

        Ok(Self {
            storage,
            embedder,
            line_chunker,
            ast_chunker,
            walker,
            config,
            error_collector,
            semaphore,
        })
    }

    /// Index files using parallel processing pipeline
    pub async fn index_files(&self, files: Vec<PathBuf>) -> Result<ProcessingResult> {
        let start = Instant::now();
        let total_files = files.len();

        info!("Starting parallel indexing of {} files", total_files);

        // Stage 1: Filter files needing indexing (sequential)
        let files_to_index = self.filter_modified_files(files).await?;

        if files_to_index.is_empty() {
            info!("No files need indexing");
            return Ok(ProcessingResult::new());
        }

        info!("{} files need indexing", files_to_index.len());

        // Create progress tracking
        let multi_progress = MultiProgress::new();
        let file_pb = self.create_progress_bar(&multi_progress, files_to_index.len(), "Files");
        let chunk_pb = self.create_progress_bar(&multi_progress, 100, "Chunks");

        // Stage 2: Parallel file reading
        let file_contents = self.read_files_parallel(
            files_to_index.clone(),
            &file_pb,
        ).await?;

        // Stage 3: Parallel chunking
        let raw_chunks = self.chunk_files_parallel(file_contents, &chunk_pb).await?;
        chunk_pb.set_length(raw_chunks.len() as u64);

        if raw_chunks.is_empty() {
            info!("No chunks generated");
            file_pb.finish_with_message("No chunks to process");
            chunk_pb.finish_with_message("No chunks");
            return Ok(ProcessingResult::new());
        }

        // Stage 4: Batch embedding generation
        chunk_pb.set_position(0);
        chunk_pb.set_message("Generating embeddings...");
        let embeddings = self.generate_embeddings_batch(&raw_chunks, &chunk_pb).await?;

        // Stage 5: Parallel assembly of indexed chunks
        chunk_pb.set_message("Assembling chunks...");
        let indexed_chunks = self.assemble_chunks_parallel(raw_chunks, embeddings).await?;

        // Stage 6: Storage with backpressure
        chunk_pb.set_position(0);
        chunk_pb.set_message("Storing chunks...");
        let chunk_count = indexed_chunks.len();
        self.store_chunks_with_backpressure(indexed_chunks.clone(), &chunk_pb).await?;

        // Finish progress bars
        file_pb.finish_with_message("Complete");
        chunk_pb.finish_with_message("Complete");

        let duration = start.elapsed();
        info!(
            "Parallel indexing completed in {:.2}s ({:.1} files/sec)",
            duration.as_secs_f64(),
            total_files as f64 / duration.as_secs_f64()
        );

        // Collect results
        let errors = self.error_collector.get_report();
        if errors.has_errors() {
            errors.print_summary();
        }

        Ok(ProcessingResult {
            successful: indexed_chunks,
            errors: errors.by_stage.into_values().flatten().collect(),
            files_processed: files_to_index.len(),
            chunks_created: chunk_count,
        })
    }

    /// Filter files that need indexing based on modification time
    async fn filter_modified_files(&self, files: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
        let existing_mtimes = self.storage.get_file_mtimes().await?;

        Ok(files
            .into_iter()
            .filter(|path| {
                let current_mtime = get_file_mtime(path).unwrap_or(0);

                if let Some(&stored_mtime) = existing_mtimes.get(path) {
                    current_mtime > stored_mtime
                } else {
                    true // New file
                }
            })
            .collect())
    }

    /// Read files in parallel using spawn_blocking
    async fn read_files_parallel(
        &self,
        files: Vec<PathBuf>,
        progress: &ProgressBar,
    ) -> Result<Vec<FileContent>> {
        let batch_size = self.config.indexer.file_batch_size;
        let mut all_contents = Vec::new();
        let processed = Arc::new(AtomicUsize::new(0));

        for batch in files.chunks(batch_size) {
            let batch = batch.to_vec();
            let error_collector = self.error_collector.clone();
            let processed_clone = processed.clone();

            let contents = tokio::task::spawn_blocking(move || {
                batch
                    .par_iter()
                    .filter_map(|path| {
                        match fs::read_to_string(path) {
                            Ok(content) => {
                                let mtime = get_file_mtime(path).unwrap_or(0);
                                processed_clone.fetch_add(1, Ordering::Relaxed);
                                Some(FileContent {
                                    path: path.clone(),
                                    content,
                                    mtime,
                                })
                            }
                            Err(e) => {
                                error_collector.record(
                                    path.clone(),
                                    e.into(),
                                    ProcessingStage::FileRead,
                                );
                                processed_clone.fetch_add(1, Ordering::Relaxed);
                                None
                            }
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .await?;

            all_contents.extend(contents);
            progress.set_position(processed.load(Ordering::Relaxed) as u64);
        }

        Ok(all_contents)
    }

    /// Chunk files in parallel
    async fn chunk_files_parallel(
        &self,
        files: Vec<FileContent>,
        progress: &ProgressBar,
    ) -> Result<Vec<RawChunk>> {
        let line_chunker = self.line_chunker.clone();
        let ast_chunker = self.ast_chunker.clone();
        let error_collector = self.error_collector.clone();

        progress.set_message("Chunking files...");

        let result = tokio::task::spawn_blocking(move || {
            files
                .par_iter()
                .flat_map(|file| {
                    // Extract file header (first 50 lines)
                    let file_header = extract_file_header(&file.content, 50);

                    match std::panic::catch_unwind(|| {
                        if let Some(ref ast_chunker) = ast_chunker {
                            let mut chunker = ast_chunker.lock().unwrap();
                            chunker.chunk_file(&file.path, &file.content)
                        } else if let Some(ref line_chunker) = line_chunker {
                            line_chunker.chunk_file(&file.path, &file.content)
                        } else {
                            Vec::new()
                        }
                    }) {
                        Ok(chunks) => {
                            chunks
                                .into_par_iter()
                                .map(|chunk| RawChunk {
                                    content: chunk.content,
                                    file_path: file.path.to_string_lossy().to_string(),
                                    start_line: chunk.start_line,
                                    end_line: chunk.end_line,
                                    language: chunk.language,
                                    mtime: file.mtime,
                                    file_header: file_header.clone(),
                                    semantic_kind: chunk.semantic_kind.map(|k| k.as_str().to_string()),
                                    symbol_name: chunk.name,
                                    signature: chunk.signature,
                                    parent: chunk.parent,
                                    visibility: None, // TODO: Extract from AST
                                })
                                .collect::<Vec<_>>()
                        }
                        Err(_) => {
                            error_collector.record(
                                file.path.clone(),
                                anyhow::anyhow!("Panic during chunking"),
                                ProcessingStage::Chunking,
                            );
                            Vec::new()
                        }
                    }
                })
                .collect::<Vec<_>>()
        })
        .await?;

        Ok(result)
    }

    /// Generate embeddings in batches
    async fn generate_embeddings_batch(
        &self,
        chunks: &[RawChunk],
        progress: &ProgressBar,
    ) -> Result<Vec<Vec<f32>>> {
        let batch_size = self.config.embeddings.batch_size;
        let mut all_embeddings = Vec::new();
        let total_chunks = chunks.len();
        let mut processed = 0;

        // Collect all chunk content
        let contents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();

        // Process in batches using the async embed method to avoid runtime nesting
        for batch in contents.chunks(batch_size * 10) {
            let batch_vec: Vec<String> = batch.to_vec();
            match self.embedder.embed_async(&batch_vec).await {
                Ok(embeddings) => {
                    processed += embeddings.len();
                    progress.set_position(processed as u64);
                    progress.set_message(format!(
                        "Generating embeddings ({}/{})",
                        processed, total_chunks
                    ));
                    all_embeddings.extend(embeddings);
                }
                Err(e) => {
                    error!("Failed to generate embeddings for batch: {}", e);
                    // Generate zero embeddings as fallback
                    let fallback_count = batch.len();
                    for _ in batch {
                        all_embeddings.push(vec![0.0; 768]); // Assuming 768-dim embeddings
                    }
                    processed += fallback_count;
                    progress.set_position(processed as u64);
                    progress.set_message(format!(
                        "Generating embeddings ({}/{})",
                        processed, total_chunks
                    ));
                }
            }
        }

        Ok(all_embeddings)
    }

    /// Assemble chunks with embeddings in parallel
    async fn assemble_chunks_parallel(
        &self,
        chunks: Vec<RawChunk>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<Vec<IndexedChunk>> {
        if chunks.len() != embeddings.len() {
            return Err(anyhow::anyhow!(
                "Chunk count {} doesn't match embedding count {}",
                chunks.len(),
                embeddings.len()
            ));
        }

        let result = tokio::task::spawn_blocking(move || {
            chunks
                .into_par_iter()
                .zip(embeddings.into_par_iter())
                .map(|(chunk, embedding)| IndexedChunk {
                    id: uuid::Uuid::new_v4().to_string(),
                    content: chunk.content.clone(),
                    file_path: chunk.file_path,
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    language: chunk.language,
                    vector: embedding,
                    mtime: chunk.mtime,
                    file_header: Some(chunk.file_header),
                    // Symbol metadata fields passed through from RawChunk
                    semantic_kind: chunk.semantic_kind,
                    symbol_name: chunk.symbol_name,
                    signature: chunk.signature,
                    parent: chunk.parent,
                    visibility: chunk.visibility,
                })
                .collect::<Vec<_>>()
        })
        .await?;

        Ok(result)
    }

    /// Store chunks with backpressure control
    async fn store_chunks_with_backpressure(
        &self,
        chunks: Vec<IndexedChunk>,
        progress: &ProgressBar,
    ) -> Result<()> {
        let batch_size = self.config.embeddings.batch_size * 10;
        let total_chunks = chunks.len();
        let mut stored = 0;

        // First, delete existing chunks for modified files
        let files: Vec<PathBuf> = chunks
            .iter()
            .map(|c| PathBuf::from(&c.file_path))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for file in &files {
            self.storage.delete_by_file(file).await?;
        }

        // Store in batches
        for batch in chunks.chunks(batch_size) {
            let _permit = self.semaphore.acquire().await?;

            self.storage
                .insert_chunks(batch.to_vec())
                .await
                .context("Failed to insert chunk batch")?;

            stored += batch.len();
            progress.set_position((stored * 100 / total_chunks) as u64);
        }

        Ok(())
    }

    /// Create a progress bar with standard styling
    fn create_progress_bar(
        &self,
        multi: &MultiProgress,
        total: usize,
        label: &str,
    ) -> ProgressBar {
        let pb = multi.add(ProgressBar::new(total as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "{{spinner:.green}} [{{elapsed_precise}}] {}: [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {{msg}}",
                    label
                ))
                .unwrap()
                .progress_chars("#>-"),
        );
        pb
    }
}

/// Get the modification time of a file as Unix timestamp
fn get_file_mtime(path: &std::path::Path) -> Result<i64> {
    let metadata = fs::metadata(path)?;
    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(mtime)
}

/// Extract the first N lines of a file as the header for context
fn extract_file_header(content: &str, max_lines: usize) -> String {
    content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}