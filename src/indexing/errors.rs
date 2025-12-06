//! Error collection and reporting for parallel indexing

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Stage where an error occurred during processing
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum ProcessingStage {
    FileRead,
    Chunking,
    Embedding,
    Storage,
}

impl std::fmt::Display for ProcessingStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingStage::FileRead => write!(f, "File Read"),
            ProcessingStage::Chunking => write!(f, "Chunking"),
            ProcessingStage::Embedding => write!(f, "Embedding"),
            ProcessingStage::Storage => write!(f, "Storage"),
        }
    }
}

/// Error that occurred while processing a file
#[derive(Debug, Clone)]
pub struct FileError {
    pub path: PathBuf,
    pub error: String,
    pub stage: ProcessingStage,
}

/// Collects errors during parallel processing
#[derive(Clone)]
pub struct ErrorCollector {
    errors: Arc<Mutex<Vec<FileError>>>,
    max_errors: usize,
}

impl ErrorCollector {
    /// Create a new error collector with a maximum error threshold
    pub fn new(max_errors: usize) -> Self {
        Self {
            errors: Arc::new(Mutex::new(Vec::new())),
            max_errors,
        }
    }

    /// Record an error that occurred during processing
    pub fn record(&self, path: PathBuf, error: anyhow::Error, stage: ProcessingStage) {
        let mut errors = self.errors.lock().unwrap();
        errors.push(FileError {
            path,
            error: error.to_string(),
            stage,
        });
    }

    /// Check if processing should continue based on error count
    pub fn should_continue(&self) -> bool {
        self.errors.lock().unwrap().len() < self.max_errors
    }

    /// Get the current error count
    pub fn error_count(&self) -> usize {
        self.errors.lock().unwrap().len()
    }

    /// Generate an error report
    pub fn get_report(&self) -> ErrorReport {
        let errors = self.errors.lock().unwrap();
        ErrorReport::from_errors(&errors)
    }

    /// Clear all collected errors
    pub fn clear(&self) {
        self.errors.lock().unwrap().clear();
    }
}

/// Detailed error report with statistics
pub struct ErrorReport {
    pub total_errors: usize,
    pub by_stage: HashMap<ProcessingStage, Vec<FileError>>,
    pub summary: String,
}

impl ErrorReport {
    /// Create a report from a list of errors
    pub fn from_errors(errors: &[FileError]) -> Self {
        let mut by_stage: HashMap<ProcessingStage, Vec<FileError>> = HashMap::new();

        for error in errors {
            by_stage
                .entry(error.stage.clone())
                .or_default()
                .push(error.clone());
        }

        let summary = if errors.is_empty() {
            "No errors occurred during processing".to_string()
        } else {
            format!("Processing completed with {} errors", errors.len())
        };

        Self {
            total_errors: errors.len(),
            by_stage,
            summary,
        }
    }

    /// Print a summary of the errors to stdout
    pub fn print_summary(&self) {
        if self.total_errors == 0 {
            println!("✅ {}", self.summary);
            return;
        }

        println!("⚠️  {}", self.summary);
        println!();

        for (stage, errors) in &self.by_stage {
            println!("  {}: {} errors", stage, errors.len());

            // Show up to 5 examples per stage
            for error in errors.iter().take(5) {
                println!("    - {}: {}", error.path.display(), error.error);
            }

            if errors.len() > 5 {
                println!("    ... and {} more", errors.len() - 5);
            }
        }
    }

    /// Check if any errors occurred
    pub fn has_errors(&self) -> bool {
        self.total_errors > 0
    }
}