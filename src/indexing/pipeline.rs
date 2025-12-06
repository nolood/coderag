//! Pipeline data structures for parallel processing

use std::path::PathBuf;
use crate::storage::IndexedChunk;
use super::errors::FileError;

/// File content with metadata
#[derive(Debug, Clone)]
pub struct FileContent {
    pub path: PathBuf,
    pub content: String,
    pub mtime: i64,
}

/// Raw chunk before embedding generation
#[derive(Debug, Clone)]
pub struct RawChunk {
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
    pub mtime: i64,
    pub file_header: String,
    // Symbol metadata
    pub semantic_kind: Option<String>,
    pub symbol_name: Option<String>,
    pub signature: Option<String>,
    pub parent: Option<String>,
    pub visibility: Option<String>,
}

/// Result of processing a batch of files
#[derive(Debug, Default)]
pub struct ProcessingResult {
    pub successful: Vec<IndexedChunk>,
    pub errors: Vec<FileError>,
    pub files_processed: usize,
    pub chunks_created: usize,
}

impl ProcessingResult {
    /// Create a new empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge another result into this one
    pub fn merge(&mut self, other: ProcessingResult) {
        self.successful.extend(other.successful);
        self.errors.extend(other.errors);
        self.files_processed += other.files_processed;
        self.chunks_created += other.chunks_created;
    }

    /// Check if processing was successful (no errors)
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get a summary string of the results
    pub fn summary(&self) -> String {
        if self.errors.is_empty() {
            format!(
                "Successfully processed {} files, created {} chunks",
                self.files_processed, self.chunks_created
            )
        } else {
            format!(
                "Processed {} files with {} errors, created {} chunks",
                self.files_processed,
                self.errors.len(),
                self.chunks_created
            )
        }
    }
}