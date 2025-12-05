//! Project metadata and statistics for multi-project support.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Statistics about a project's index.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectStats {
    /// Number of indexed files
    pub file_count: usize,
    /// Number of chunks in the index
    pub chunk_count: usize,
    /// Size of the index in bytes
    pub index_size_bytes: u64,
}

impl ProjectStats {
    /// Create new project statistics.
    pub fn new(file_count: usize, chunk_count: usize, index_size_bytes: u64) -> Self {
        Self {
            file_count,
            chunk_count,
            index_size_bytes,
        }
    }
}

/// Metadata about a registered project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Human-readable project name
    pub name: String,
    /// Canonical path to project root
    pub path: PathBuf,
    /// When the project was first registered
    pub created_at: DateTime<Utc>,
    /// When the project was last indexed
    pub last_indexed: Option<DateTime<Utc>>,
    /// Index statistics (if available)
    pub stats: Option<ProjectStats>,
}

impl ProjectInfo {
    /// Create a new project info with the given name and path.
    pub fn new(name: String, path: PathBuf) -> Self {
        Self {
            name,
            path,
            created_at: Utc::now(),
            last_indexed: None,
            stats: None,
        }
    }

    /// Update the last indexed timestamp to now.
    pub fn mark_indexed(&mut self) {
        self.last_indexed = Some(Utc::now());
    }

    /// Update the project statistics.
    pub fn update_stats(&mut self, stats: ProjectStats) {
        self.stats = Some(stats);
    }

    /// Check if the project path exists on disk.
    pub fn path_exists(&self) -> bool {
        self.path.exists()
    }

    /// Get a display-friendly representation of the project path.
    pub fn display_path(&self) -> String {
        self.path.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_project_info_new() {
        let project = ProjectInfo::new("test-project".to_string(), PathBuf::from("/tmp/test"));
        assert_eq!(project.name, "test-project");
        assert_eq!(project.path, PathBuf::from("/tmp/test"));
        assert!(project.last_indexed.is_none());
        assert!(project.stats.is_none());
    }

    #[test]
    fn test_project_info_mark_indexed() {
        let mut project = ProjectInfo::new("test".to_string(), PathBuf::from("/tmp"));
        assert!(project.last_indexed.is_none());

        project.mark_indexed();
        assert!(project.last_indexed.is_some());
    }

    #[test]
    fn test_project_stats() {
        let stats = ProjectStats::new(10, 100, 1024);
        assert_eq!(stats.file_count, 10);
        assert_eq!(stats.chunk_count, 100);
        assert_eq!(stats.index_size_bytes, 1024);
    }

    #[test]
    fn test_path_exists() {
        let dir = tempdir().unwrap();
        let project = ProjectInfo::new("test".to_string(), dir.path().to_path_buf());
        assert!(project.path_exists());

        let missing_project = ProjectInfo::new("missing".to_string(), PathBuf::from("/nonexistent/path"));
        assert!(!missing_project.path_exists());
    }
}
