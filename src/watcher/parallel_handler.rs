//! Parallel change handler for processing file system changes concurrently
//!
//! This module handles the actual re-indexing of files when changes are detected,
//! using parallel processing for improved performance.

use anyhow::{Context, Result};
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::indexer::Chunker;
use crate::storage::{IndexedChunk, Storage};

use super::debouncer::{ChangeType, FileChange};
use super::handler::ProcessingStats;

/// Handles file changes and triggers re-indexing using parallel processing
#[derive(Clone)]
pub struct ParallelChangeHandler {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    chunker: Arc<Chunker>,
    semaphore: Arc<Semaphore>,
    #[allow(dead_code)]
    root: PathBuf,
    #[allow(dead_code)]
    config: Config,
}

impl ParallelChangeHandler {
    /// Create a new parallel change handler
    pub fn new(
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        root: PathBuf,
        config: Config,
    ) -> Result<Self> {
        let chunker = Arc::new(Chunker::new(config.indexer.chunk_size));
        let semaphore = Arc::new(Semaphore::new(config.indexer.max_concurrent_files));

        Ok(Self {
            storage,
            embedder,
            chunker,
            semaphore,
            root,
            config,
        })
    }

    /// Process changes concurrently with controlled parallelism
    pub async fn process_changes_concurrent(
        &self,
        changes: Vec<FileChange>,
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

    /// Process a single file change
    async fn process_single_change(&self, change: FileChange) -> Result<ProcessingStats> {
        let mut stats = ProcessingStats::default();

        debug!(
            "Processing {} change for {:?}",
            change.change_type, change.path
        );

        match change.change_type {
            ChangeType::Created => {
                stats.chunks_created = self.index_file(&change.path).await?;
                stats.files_added = 1;
                info!("Indexed new file: {:?}", change.path);
            }
            ChangeType::Modified => {
                // Delete existing chunks first
                stats.chunks_removed = self.delete_file_chunks(&change.path).await?;
                // Then re-index
                stats.chunks_created = self.index_file(&change.path).await?;
                stats.files_modified = 1;
                info!("Re-indexed modified file: {:?}", change.path);
            }
            ChangeType::Deleted => {
                stats.chunks_removed = self.delete_file_chunks(&change.path).await?;
                stats.files_deleted = 1;
                info!("Removed deleted file from index: {:?}", change.path);
            }
        }

        Ok(stats)
    }

    /// Index a single file with parallel chunking
    async fn index_file(&self, path: &PathBuf) -> Result<usize> {
        // Read file content
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                warn!("Could not read file {:?}: {}", path, e);
                return Ok(0);
            }
        };

        if content.trim().is_empty() {
            debug!("Skipping empty file: {:?}", path);
            return Ok(0);
        }

        // Get file mtime
        let mtime = get_file_mtime(path).unwrap_or(0);

        // Extract file header (first 50 lines)
        let file_header = extract_file_header(&content, 50);

        // Chunk the file
        let chunks = self.chunker.chunk_file(path, &content);

        if chunks.is_empty() {
            debug!("No chunks generated for file: {:?}", path);
            return Ok(0);
        }

        // Prepare chunks for embedding
        let chunk_contents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();

        // Generate embeddings
        let embeddings = self
            .embedder
            .embed(&chunk_contents)
            .with_context(|| format!("Failed to generate embeddings for {:?}", path))?;

        // Create indexed chunks in parallel
        let file_path_str = path.to_string_lossy().to_string();
        let indexed_chunks: Vec<IndexedChunk> = chunks
            .into_par_iter()
            .zip(embeddings.into_par_iter())
            .map(|(chunk, embedding)| IndexedChunk {
                id: uuid::Uuid::new_v4().to_string(),
                content: chunk.content,
                file_path: file_path_str.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                language: chunk.language,
                vector: embedding,
                mtime,
                file_header: Some(file_header.clone()),
                semantic_kind: chunk.semantic_kind.map(|k| k.as_str().to_string()),
                symbol_name: chunk.name,
                signature: chunk.signature,
                parent: chunk.parent,
                visibility: None, // TODO: Extract from AST
            })
            .collect();

        let chunk_count = indexed_chunks.len();

        // Insert chunks
        self.storage
            .insert_chunks(indexed_chunks)
            .await
            .with_context(|| format!("Failed to insert chunks for {:?}", path))?;

        Ok(chunk_count)
    }

    /// Delete all chunks for a file
    async fn delete_file_chunks(&self, path: &PathBuf) -> Result<usize> {
        self.storage
            .delete_by_file(path)
            .await
            .with_context(|| format!("Failed to delete chunks for {:?}", path))?;

        // Return 1 as a placeholder since we removed at least some chunks
        Ok(1)
    }
}

/// Batched event processor for handling file system events efficiently
pub struct BatchedEventProcessor {
    batch_timeout: std::time::Duration,
    batch_size: usize,
    handler: Arc<ParallelChangeHandler>,
}

impl BatchedEventProcessor {
    /// Create a new batched event processor
    pub fn new(handler: Arc<ParallelChangeHandler>) -> Self {
        Self {
            batch_timeout: std::time::Duration::from_millis(100),
            batch_size: 50,
            handler,
        }
    }

    /// Process an event stream with batching
    pub async fn process_event_stream(
        &self,
        mut rx: tokio::sync::mpsc::Receiver<FileChange>,
    ) {
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

    /// Flush the current batch for processing
    async fn flush_batch(&self, batch: &mut Vec<FileChange>) {
        if batch.is_empty() {
            return;
        }

        let changes = std::mem::take(batch);
        if let Err(e) = self.handler.process_changes_concurrent(changes).await {
            error!("Batch processing failed: {}", e);
        }
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