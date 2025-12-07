//! Project detection module for zero-ceremony auto-indexing.
//!
//! This module provides utilities to detect project roots by traversing
//! up the directory tree looking for project markers like `.git`, `Cargo.toml`,
//! `package.json`, etc.
//!
//! # Overview
//!
//! The project detection system enables CodeRAG's "zero-ceremony" feature,
//! allowing users to run `coderag search "query"` from any subdirectory
//! without explicit initialization. The system automatically detects the
//! project root and uses global storage for indexes.
//!
//! # Detection Algorithm
//!
//! 1. Start from the current working directory
//! 2. Check if `.coderag` directory exists (highest priority - backward compat)
//! 3. Check for other project markers (`.git`, `Cargo.toml`, etc.)
//! 4. Move to parent directory and repeat
//! 5. Stop when a marker is found or filesystem root is reached
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use coderag::project_detection::{ProjectDetector, ProjectType};
//!
//! let detector = ProjectDetector::default();
//! let project = detector.detect(Path::new("/home/user/myproject/src/lib")).unwrap();
//!
//! println!("Project root: {}", project.root.display());
//! println!("Project type: {}", project.project_type);
//! println!("Has local config: {}", project.has_local_config);
//! ```
//!
//! # Backward Compatibility
//!
//! Projects with an existing `.coderag` directory will continue to use local
//! storage. The `.coderag` marker has the highest priority (priority 0) and
//! is always checked first at each directory level.

mod detector;
mod markers;

// Re-export main types
pub use detector::{DetectedProject, DetectionError, ProjectDetector, MAX_TRAVERSAL_DEPTH};
pub use markers::{coderag_marker, find_marker, ProjectMarker, ProjectType, DEFAULT_MARKERS};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Integration test: detect project from subdirectory
    #[test]
    fn test_integration_detect_from_subdirectory() {
        let root = tempdir().unwrap();

        // Create a Rust project structure
        std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();

        // Create nested source directories
        let src_dir = root.path().join("src").join("utils").join("helpers");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("mod.rs"), "// helpers").unwrap();

        // Detect from the deeply nested directory
        let detector = ProjectDetector::default();
        let project = detector.detect(&src_dir).unwrap();

        assert_eq!(project.root, root.path().canonicalize().unwrap());
        // .git is detected before Cargo.toml due to priority
        assert_eq!(project.project_type, ProjectType::Git);
    }

    /// Integration test: .coderag takes priority
    #[test]
    fn test_integration_coderag_priority() {
        let root = tempdir().unwrap();

        // Create both .coderag and .git
        std::fs::create_dir(root.path().join(".coderag")).unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();

        let detector = ProjectDetector::default();
        let project = detector.detect(root.path()).unwrap();

        assert!(project.has_local_config);
        assert_eq!(project.marker.name, ".coderag");
    }
}
