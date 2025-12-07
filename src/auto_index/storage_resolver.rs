//! Storage resolution for auto-indexing.
//!
//! Resolves where to store indexes for a project - either locally in `.coderag/`
//! for explicitly initialized projects, or globally in `~/.local/share/coderag/indexes/`.

use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::project_detection::DetectedProject;
use crate::registry::GlobalRegistry;

/// Errors during storage resolution.
#[derive(Error, Debug)]
pub enum StorageError {
    /// Failed to compute project ID.
    #[error("Failed to compute project ID: {reason}")]
    ProjectIdError {
        /// The reason for the failure
        reason: String,
    },

    /// Failed to access global directory.
    #[error("Failed to access global directory: {0}")]
    GlobalDirError(#[from] anyhow::Error),

    /// I/O error during storage operations.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Where the index for a project is stored.
#[derive(Debug, Clone)]
pub enum StorageLocation {
    /// Local storage in .coderag/ directory (existing projects)
    Local {
        /// Project root path
        root: PathBuf,
        /// Path to LanceDB database
        db_path: PathBuf,
        /// Path to BM25 index directory
        bm25_path: PathBuf,
    },
    /// Global storage in ~/.local/share/coderag/indexes/{project_id}/
    Global {
        /// Project root path
        root: PathBuf,
        /// Unique project identifier
        project_id: String,
        /// Path to LanceDB database
        db_path: PathBuf,
        /// Path to BM25 index directory
        bm25_path: PathBuf,
    },
}

impl StorageLocation {
    /// Get the LanceDB database path.
    pub fn db_path(&self) -> &Path {
        match self {
            Self::Local { db_path, .. } => db_path,
            Self::Global { db_path, .. } => db_path,
        }
    }

    /// Get the BM25 index path.
    pub fn bm25_path(&self) -> &Path {
        match self {
            Self::Local { bm25_path, .. } => bm25_path,
            Self::Global { bm25_path, .. } => bm25_path,
        }
    }

    /// Get the project root path.
    pub fn root(&self) -> &Path {
        match self {
            Self::Local { root, .. } => root,
            Self::Global { root, .. } => root,
        }
    }

    /// Check if this is local storage.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    /// Check if the index exists at this location.
    pub fn index_exists(&self) -> bool {
        self.db_path().exists()
    }

    /// Get the storage directory (parent of db_path).
    pub fn storage_dir(&self) -> Option<&Path> {
        self.db_path().parent()
    }
}

/// Resolves where to store/find indexes for a project.
pub struct StorageResolver;

impl StorageResolver {
    /// Resolve storage location for a detected project.
    ///
    /// Priority:
    /// 1. If `.coderag/` exists locally, use local storage (backward compat)
    /// 2. Otherwise, use global storage
    pub fn resolve(project: &DetectedProject) -> Result<StorageLocation, StorageError> {
        if project.has_local_config {
            Self::resolve_local(&project.root)
        } else {
            Self::resolve_global(&project.root)
        }
    }

    /// Resolve to local `.coderag/` storage.
    fn resolve_local(root: &Path) -> Result<StorageLocation, StorageError> {
        let coderag_dir = root.join(".coderag");
        Ok(StorageLocation::Local {
            root: root.to_path_buf(),
            db_path: coderag_dir.join("index.lance"),
            bm25_path: coderag_dir.join("bm25"),
        })
    }

    /// Resolve to global `~/.local/share/coderag/indexes/` storage.
    fn resolve_global(root: &Path) -> Result<StorageLocation, StorageError> {
        let project_id = compute_project_id(root)?;
        let global_dir = GlobalRegistry::global_dir()?;
        let index_dir = global_dir.join("indexes").join(&project_id);

        Ok(StorageLocation::Global {
            root: root.to_path_buf(),
            project_id,
            db_path: index_dir.join("index.lance"),
            bm25_path: index_dir.join("bm25"),
        })
    }
}

/// Compute a stable, unique project ID from the root path.
///
/// Uses a combination of:
/// - Directory name (for readability)
/// - Hash of canonical path (for uniqueness)
///
/// # Format
///
/// `{sanitized-name}-{path-hash}` where:
/// - `sanitized-name` is lowercase alphanumeric + hyphens/underscores
/// - `path-hash` is an 8-character hex hash of the canonical path
///
/// # Examples
///
/// ```text
/// /home/user/projects/coderag -> coderag-a1b2c3d4
/// /home/user/work/my-app -> my-app-e5f6g7h8
/// ```
pub fn compute_project_id(root: &Path) -> Result<String, StorageError> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let canonical = root.canonicalize().map_err(|e| StorageError::ProjectIdError {
        reason: format!("Failed to canonicalize path: {}", e),
    })?;

    // Get directory name for readability
    let dir_name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project");

    // Hash the full path for uniqueness
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    let hash = hasher.finish();

    // Combine: "project-name-abc12345" (8 hex chars = 32 bits)
    Ok(format!("{}-{:08x}", sanitize_name(dir_name), hash as u32))
}

/// Sanitize a directory name for use in file paths.
///
/// Converts to lowercase and replaces non-alphanumeric characters
/// (except hyphens and underscores) with hyphens.
///
/// # Examples
///
/// ```
/// use coderag::auto_index::sanitize_name;
///
/// assert_eq!(sanitize_name("My Project"), "my-project");
/// assert_eq!(sanitize_name("test_project"), "test_project");
/// assert_eq!(sanitize_name("foo@bar!baz"), "foo-bar-baz");
/// ```
pub fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_project_id_stability() {
        let dir = tempdir().unwrap();

        let id1 = compute_project_id(dir.path()).unwrap();
        let id2 = compute_project_id(dir.path()).unwrap();

        assert_eq!(id1, id2, "Project ID should be stable");
    }

    #[test]
    fn test_project_id_format() {
        let dir = tempdir().unwrap();
        let id = compute_project_id(dir.path()).unwrap();

        // Should contain a hyphen separating name and hash
        assert!(id.contains('-'), "Project ID should contain hyphen");

        // Should end with 8 hex characters
        let parts: Vec<&str> = id.rsplitn(2, '-').collect();
        assert!(parts.len() >= 1);
        assert_eq!(parts[0].len(), 8);
        assert!(parts[0].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_local_storage_paths() {
        let dir = tempdir().unwrap();
        let storage = StorageResolver::resolve_local(dir.path()).unwrap();

        match storage {
            StorageLocation::Local {
                root,
                db_path,
                bm25_path,
            } => {
                assert_eq!(root, dir.path());
                assert_eq!(db_path, dir.path().join(".coderag/index.lance"));
                assert_eq!(bm25_path, dir.path().join(".coderag/bm25"));
            }
            _ => panic!("Expected Local storage"),
        }
    }

    #[test]
    fn test_global_storage_paths() {
        let dir = tempdir().unwrap();
        let storage = StorageResolver::resolve_global(dir.path()).unwrap();

        match storage {
            StorageLocation::Global {
                root,
                project_id,
                db_path,
                bm25_path,
            } => {
                assert_eq!(root, dir.path().canonicalize().unwrap());
                assert!(!project_id.is_empty());
                assert!(db_path.to_string_lossy().contains("indexes"));
                assert!(db_path.to_string_lossy().contains(&project_id));
                assert!(bm25_path.to_string_lossy().contains(&project_id));
            }
            _ => panic!("Expected Global storage"),
        }
    }

    #[test]
    fn test_index_exists() {
        let dir = tempdir().unwrap();
        let storage = StorageResolver::resolve_local(dir.path()).unwrap();

        // Index should not exist initially
        assert!(!storage.index_exists());

        // Create the index directory
        std::fs::create_dir_all(storage.db_path()).unwrap();

        // Now it should exist
        assert!(storage.index_exists());
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("my-project"), "my-project");
        assert_eq!(sanitize_name("my_project"), "my_project");
        assert_eq!(sanitize_name("MyProject"), "myproject");
        assert_eq!(sanitize_name("my project"), "my-project");
        assert_eq!(sanitize_name("my.project"), "my-project");
        assert_eq!(sanitize_name("my/project"), "my-project");
    }
}
