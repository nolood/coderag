//! Event debouncing for file system changes
//!
//! This module provides types for representing and tracking file changes
//! with debouncing support.

use std::path::PathBuf;
use std::time::Instant;

/// Types of file system changes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeType {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeType::Created => write!(f, "created"),
            ChangeType::Modified => write!(f, "modified"),
            ChangeType::Deleted => write!(f, "deleted"),
        }
    }
}

/// A file change event
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path to the changed file
    pub path: PathBuf,
    /// Type of change
    pub change_type: ChangeType,
    /// When the change was detected
    pub timestamp: Instant,
}

impl FileChange {
    /// Create a new file change
    pub fn new(path: PathBuf, change_type: ChangeType) -> Self {
        Self {
            path,
            change_type,
            timestamp: Instant::now(),
        }
    }

    /// Create a new Created change
    pub fn created(path: PathBuf) -> Self {
        Self::new(path, ChangeType::Created)
    }

    /// Create a new Modified change
    pub fn modified(path: PathBuf) -> Self {
        Self::new(path, ChangeType::Modified)
    }

    /// Create a new Deleted change
    pub fn deleted(path: PathBuf) -> Self {
        Self::new(path, ChangeType::Deleted)
    }

    /// Check if this change requires file content (i.e., not a deletion)
    pub fn needs_content(&self) -> bool {
        !matches!(self.change_type, ChangeType::Deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_type_display() {
        assert_eq!(format!("{}", ChangeType::Created), "created");
        assert_eq!(format!("{}", ChangeType::Modified), "modified");
        assert_eq!(format!("{}", ChangeType::Deleted), "deleted");
    }

    #[test]
    fn test_file_change_creation() {
        let path = PathBuf::from("/test/file.rs");

        let created = FileChange::created(path.clone());
        assert_eq!(created.change_type, ChangeType::Created);
        assert!(created.needs_content());

        let modified = FileChange::modified(path.clone());
        assert_eq!(modified.change_type, ChangeType::Modified);
        assert!(modified.needs_content());

        let deleted = FileChange::deleted(path);
        assert_eq!(deleted.change_type, ChangeType::Deleted);
        assert!(!deleted.needs_content());
    }
}
