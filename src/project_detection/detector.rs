//! Project root detection logic.
//!
//! This module implements the core detection algorithm that walks up the
//! directory tree looking for project markers. The `.coderag` directory
//! takes highest priority for backward compatibility.

use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, trace};

use super::markers::{coderag_marker, ProjectMarker, ProjectType, DEFAULT_MARKERS};

/// Maximum number of directories to traverse upward.
pub const MAX_TRAVERSAL_DEPTH: usize = 100;

/// Errors that can occur during project detection.
#[derive(Error, Debug)]
pub enum DetectionError {
    /// No project root was found after traversing the directory tree.
    #[error("No project root found from {starting_dir}")]
    NoProjectRoot { starting_dir: PathBuf },

    /// An I/O error occurred during detection.
    #[error("IO error during detection: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to canonicalize a path.
    #[error("Path canonicalization failed for {path}: {source}")]
    Canonicalization {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Result of successful project detection.
#[derive(Debug, Clone)]
pub struct DetectedProject {
    /// Canonical path to project root
    pub root: PathBuf,
    /// The marker that identified this project
    pub marker: &'static ProjectMarker,
    /// Detected project type
    pub project_type: ProjectType,
    /// Whether this project has an existing .coderag directory
    pub has_local_config: bool,
}

impl DetectedProject {
    /// Get the marker name that identified this project.
    pub fn marker_name(&self) -> &'static str {
        self.marker.name
    }

    /// Check if this is a git repository.
    pub fn is_git_repo(&self) -> bool {
        self.project_type == ProjectType::Git || self.root.join(".git").exists()
    }

    /// Get the path to the local .coderag directory (if it exists).
    pub fn local_config_dir(&self) -> Option<PathBuf> {
        if self.has_local_config {
            Some(self.root.join(".coderag"))
        } else {
            None
        }
    }
}

/// Detects project roots by traversing up the directory tree.
///
/// The detector starts from a given directory and walks up the tree,
/// checking each directory for project markers. The first marker found
/// determines the project root.
///
/// # Priority
///
/// The `.coderag` directory is always checked first (highest priority) to ensure
/// backward compatibility with explicitly initialized projects. After that,
/// markers are checked in the order defined in `DEFAULT_MARKERS`.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use coderag::project_detection::ProjectDetector;
///
/// let detector = ProjectDetector::default();
/// let project = detector.detect(Path::new("/home/user/myproject/src")).unwrap();
/// println!("Project root: {}", project.root.display());
/// ```
pub struct ProjectDetector {
    /// Custom markers (if different from defaults)
    markers: &'static [ProjectMarker],
    /// Maximum directories to traverse upward
    max_depth: usize,
}

impl Default for ProjectDetector {
    fn default() -> Self {
        Self {
            markers: DEFAULT_MARKERS,
            max_depth: MAX_TRAVERSAL_DEPTH,
        }
    }
}

impl ProjectDetector {
    /// Create a new detector with custom settings.
    pub fn new(markers: &'static [ProjectMarker], max_depth: usize) -> Self {
        Self { markers, max_depth }
    }

    /// Create a detector with a custom maximum traversal depth.
    pub fn with_max_depth(max_depth: usize) -> Self {
        Self {
            markers: DEFAULT_MARKERS,
            max_depth,
        }
    }

    /// Detect project root starting from the given directory.
    ///
    /// Traverses upward until a project marker is found or the filesystem
    /// root is reached. Returns an error if no project marker is found
    /// within the maximum traversal depth.
    ///
    /// # Arguments
    ///
    /// * `starting_dir` - The directory to start searching from
    ///
    /// # Returns
    ///
    /// * `Ok(DetectedProject)` - Information about the detected project
    /// * `Err(DetectionError)` - If no project root could be found
    pub fn detect(&self, starting_dir: &Path) -> Result<DetectedProject, DetectionError> {
        debug!(starting_dir = %starting_dir.display(), "Starting project detection");

        let canonical = starting_dir.canonicalize().map_err(|e| {
            debug!(
                path = %starting_dir.display(),
                error = %e,
                "Failed to canonicalize starting directory"
            );
            DetectionError::Canonicalization {
                path: starting_dir.to_path_buf(),
                source: e,
            }
        })?;

        trace!(canonical_path = %canonical.display(), "Canonicalized starting directory");

        let mut current = canonical.as_path();
        let mut depth = 0;

        while depth < self.max_depth {
            trace!(
                depth,
                current_dir = %current.display(),
                "Checking directory for markers"
            );

            // Check for .coderag first (explicit initialization takes priority)
            let coderag_dir = current.join(".coderag");
            if coderag_dir.is_dir() {
                debug!(
                    root = %current.display(),
                    "Found .coderag directory (local config)"
                );

                return Ok(DetectedProject {
                    root: current.to_path_buf(),
                    marker: coderag_marker(),
                    project_type: ProjectType::Generic,
                    has_local_config: true,
                });
            }

            // Check other markers by priority order
            for marker in self.markers.iter().filter(|m| m.name != ".coderag") {
                let marker_path = current.join(marker.name);

                // Use try_exists to handle permission errors gracefully
                match marker_path.try_exists() {
                    Ok(true) => {
                        debug!(
                            root = %current.display(),
                            marker = marker.name,
                            project_type = %marker.project_type,
                            "Found project marker"
                        );

                        return Ok(DetectedProject {
                            root: current.to_path_buf(),
                            marker,
                            project_type: marker.project_type,
                            has_local_config: false,
                        });
                    }
                    Ok(false) => {
                        // Marker doesn't exist, continue checking
                    }
                    Err(e) => {
                        // Permission error or other I/O issue - log and continue
                        trace!(
                            marker = marker.name,
                            path = %marker_path.display(),
                            error = %e,
                            "Could not check marker existence"
                        );
                    }
                }
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => {
                    current = parent;
                    depth += 1;
                }
                _ => {
                    trace!("Reached filesystem root without finding project marker");
                    break;
                }
            }
        }

        if depth >= self.max_depth {
            debug!(
                max_depth = self.max_depth,
                "Reached maximum traversal depth without finding project"
            );
        }

        Err(DetectionError::NoProjectRoot {
            starting_dir: starting_dir.to_path_buf(),
        })
    }

    /// Check if a directory is a project root (contains any marker).
    pub fn is_project_root(&self, dir: &Path) -> bool {
        self.markers.iter().any(|m| dir.join(m.name).exists())
    }

    /// Find all markers present in a directory.
    pub fn find_markers(&self, dir: &Path) -> Vec<&'static ProjectMarker> {
        self.markers
            .iter()
            .filter(|m| dir.join(m.name).exists())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_git_project() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let detector = ProjectDetector::default();
        let result = detector.detect(dir.path());

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Git);
        assert!(!project.has_local_config);
        assert_eq!(project.marker_name(), ".git");
    }

    #[test]
    fn test_detect_cargo_project() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let detector = ProjectDetector::default();
        let result = detector.detect(dir.path());

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Rust);
        assert_eq!(project.marker_name(), "Cargo.toml");
    }

    #[test]
    fn test_detect_node_project() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let detector = ProjectDetector::default();
        let result = detector.detect(dir.path());

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Node);
    }

    #[test]
    fn test_detect_python_project() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[project]").unwrap();

        let detector = ProjectDetector::default();
        let result = detector.detect(dir.path());

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Python);
    }

    #[test]
    fn test_detect_go_project() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module example").unwrap();

        let detector = ProjectDetector::default();
        let result = detector.detect(dir.path());

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.project_type, ProjectType::Go);
    }

    #[test]
    fn test_detect_coderag_takes_priority() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".coderag")).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let detector = ProjectDetector::default();
        let project = detector.detect(dir.path()).unwrap();

        assert!(project.has_local_config);
        assert_eq!(project.marker_name(), ".coderag");
        assert!(project.local_config_dir().is_some());
    }

    #[test]
    fn test_detect_from_subdirectory() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        // Create nested subdirectory
        let subdir = dir.path().join("src").join("lib").join("utils");
        std::fs::create_dir_all(&subdir).unwrap();

        let detector = ProjectDetector::default();
        let result = detector.detect(&subdir);

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.root, dir.path().canonicalize().unwrap());
        assert_eq!(project.project_type, ProjectType::Git);
    }

    #[test]
    fn test_detect_from_deeply_nested_subdirectory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        // Create deeply nested subdirectory
        let mut nested = dir.path().to_path_buf();
        for i in 0..10 {
            nested = nested.join(format!("level{}", i));
        }
        std::fs::create_dir_all(&nested).unwrap();

        let detector = ProjectDetector::default();
        let result = detector.detect(&nested);

        assert!(result.is_ok());
        let project = result.unwrap();
        assert_eq!(project.root, dir.path().canonicalize().unwrap());
        assert_eq!(project.project_type, ProjectType::Rust);
    }

    #[test]
    fn test_no_project_root() {
        let dir = tempdir().unwrap();
        // Create an empty directory with no markers

        // Use a very small max_depth to ensure we don't find a project
        let detector = ProjectDetector::with_max_depth(1);
        let result = detector.detect(dir.path());

        assert!(result.is_err());
        assert!(matches!(result, Err(DetectionError::NoProjectRoot { .. })));
    }

    #[test]
    fn test_is_project_root() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let detector = ProjectDetector::default();
        assert!(detector.is_project_root(dir.path()));
    }

    #[test]
    fn test_is_not_project_root() {
        let dir = tempdir().unwrap();
        // Empty directory

        let detector = ProjectDetector::default();
        assert!(!detector.is_project_root(dir.path()));
    }

    #[test]
    fn test_find_markers() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();

        let detector = ProjectDetector::default();
        let markers = detector.find_markers(dir.path());

        assert_eq!(markers.len(), 2);
        assert!(markers.iter().any(|m| m.name == ".git"));
        assert!(markers.iter().any(|m| m.name == "Cargo.toml"));
    }

    #[test]
    fn test_detected_project_is_git_repo() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let detector = ProjectDetector::default();
        let project = detector.detect(dir.path()).unwrap();

        // Even though Cargo.toml was detected first, .git exists
        assert!(project.is_git_repo());
    }

    #[test]
    fn test_canonicalization_error() {
        let detector = ProjectDetector::default();
        let result = detector.detect(Path::new("/nonexistent/path/that/does/not/exist"));

        assert!(matches!(
            result,
            Err(DetectionError::Canonicalization { .. })
        ));
    }

    #[test]
    fn test_max_depth_limit() {
        let dir = tempdir().unwrap();
        // No markers in the temp directory

        // Use a max_depth of 0 - should fail immediately
        let detector = ProjectDetector::with_max_depth(0);
        let result = detector.detect(dir.path());

        assert!(matches!(result, Err(DetectionError::NoProjectRoot { .. })));
    }
}
