//! Change handler for processing file system changes
//!
//! This module handles the actual re-indexing of files when changes are detected.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::indexer::Chunker;
use crate::storage::{IndexedChunk, Storage};

use super::accumulator::{ChangeType, FileChange};

/// Statistics from processing file changes
#[derive(Debug, Default, Clone)]
pub struct ProcessingStats {
    /// Number of files added
    pub files_added: usize,
    /// Number of files modified
    pub files_modified: usize,
    /// Number of files deleted
    pub files_deleted: usize,
    /// Number of chunks created
    pub chunks_created: usize,
    /// Number of chunks removed
    pub chunks_removed: usize,
    /// Number of errors encountered
    pub errors: usize,
}

impl ProcessingStats {
    /// Merge another stats instance into this one
    pub fn merge(&mut self, other: &ProcessingStats) {
        self.files_added += other.files_added;
        self.files_modified += other.files_modified;
        self.files_deleted += other.files_deleted;
        self.chunks_created += other.chunks_created;
        self.chunks_removed += other.chunks_removed;
        self.errors += other.errors;
    }

    /// Check if any files were processed
    pub fn has_changes(&self) -> bool {
        self.files_added > 0 || self.files_modified > 0 || self.files_deleted > 0
    }

    /// Total number of files processed
    pub fn total_files(&self) -> usize {
        self.files_added + self.files_modified + self.files_deleted
    }
}

/// Handles file changes and triggers re-indexing
pub struct ChangeHandler {
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    chunker: Chunker,
    #[allow(dead_code)]
    root: PathBuf,
    #[allow(dead_code)]
    config: Config,
}

impl ChangeHandler {
    /// Create a new change handler
    pub fn new(
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        root: PathBuf,
        config: Config,
    ) -> Result<Self> {
        let chunker = Chunker::new(config.indexer.chunk_size);

        Ok(Self {
            storage,
            embedder,
            chunker,
            root,
            config,
        })
    }

    /// Process a batch of file changes
    ///
    /// # Arguments
    /// * `changes` - Batch of file changes to process
    ///
    /// # Returns
    /// Statistics about the processing
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

    /// Process a single file change
    async fn process_single(&mut self, change: &FileChange) -> Result<ProcessingStats> {
        let mut stats = ProcessingStats::default();

        debug!(
            "Processing {} change for {:?}",
            change.change_type, change.path
        );

        match &change.change_type {
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
            ChangeType::Renamed { from } => {
                // Delete chunks from old location
                stats.chunks_removed = self.delete_file_chunks(from).await?;
                // Index at new location
                stats.chunks_created = self.index_file(&change.path).await?;
                stats.files_modified = 1;
                info!("Re-indexed renamed file: {:?} -> {:?}", from, change.path);
            }
        }

        Ok(stats)
    }

    /// Index a single file
    ///
    /// Returns the number of chunks created
    async fn index_file(&mut self, path: &PathBuf) -> Result<usize> {
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

        // Generate embeddings using async method to avoid runtime nesting
        let embeddings = self
            .embedder
            .embed_async(&chunk_contents)
            .await
            .with_context(|| format!("Failed to generate embeddings for {:?}", path))?;

        // Create indexed chunks
        let file_path_str = path.to_string_lossy().to_string();
        let indexed_chunks: Vec<IndexedChunk> = chunks
            .iter()
            .zip(embeddings.into_iter())
            .map(|(chunk, embedding)| IndexedChunk {
                id: uuid::Uuid::new_v4().to_string(),
                content: chunk.content.clone(),
                file_path: file_path_str.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                language: chunk.language.clone(),
                vector: embedding,
                mtime,
                file_header: Some(file_header.clone()),
                semantic_kind: chunk.semantic_kind.map(|k| k.as_str().to_string()),
                symbol_name: chunk.name.clone(),
                signature: chunk.signature.clone(),
                parent: chunk.parent.clone(),
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
    ///
    /// Returns the number of chunks that were deleted (estimated)
    async fn delete_file_chunks(&self, path: &PathBuf) -> Result<usize> {
        // We don't have a way to count before deletion, so we'll just return 1
        // as a placeholder. In a real implementation, you might want to query
        // the count before deletion.
        self.storage
            .delete_by_file(path)
            .await
            .with_context(|| format!("Failed to delete chunks for {:?}", path))?;

        // Return 1 as a placeholder since we removed at least some chunks
        Ok(1)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processing_stats_merge() {
        let mut stats1 = ProcessingStats {
            files_added: 1,
            files_modified: 2,
            files_deleted: 0,
            chunks_created: 10,
            chunks_removed: 5,
            errors: 0,
        };

        let stats2 = ProcessingStats {
            files_added: 2,
            files_modified: 1,
            files_deleted: 1,
            chunks_created: 8,
            chunks_removed: 3,
            errors: 1,
        };

        stats1.merge(&stats2);

        assert_eq!(stats1.files_added, 3);
        assert_eq!(stats1.files_modified, 3);
        assert_eq!(stats1.files_deleted, 1);
        assert_eq!(stats1.chunks_created, 18);
        assert_eq!(stats1.chunks_removed, 8);
        assert_eq!(stats1.errors, 1);
    }

    #[test]
    fn test_processing_stats_has_changes() {
        let empty = ProcessingStats::default();
        assert!(!empty.has_changes());

        let with_added = ProcessingStats {
            files_added: 1,
            ..Default::default()
        };
        assert!(with_added.has_changes());
    }
}
