//! Project marker definitions for automatic project root detection.
//!
//! This module defines the markers (files and directories) used to identify
//! project roots during automatic detection. The detection system walks up
//! the directory tree looking for these markers.

/// Defines a project marker file/directory that indicates a project root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMarker {
    /// Name of the marker file or directory
    pub name: &'static str,
    /// Priority (lower = higher priority, checked first)
    pub priority: u8,
    /// Project type this marker indicates
    pub project_type: ProjectType,
}

impl ProjectMarker {
    /// Create a new project marker.
    pub const fn new(name: &'static str, priority: u8, project_type: ProjectType) -> Self {
        Self {
            name,
            priority,
            project_type,
        }
    }
}

/// Recognized project types based on detected markers.
///
/// The project type helps determine default configuration settings
/// and which file extensions to prioritize during indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectType {
    /// Git repository (most common marker)
    Git,
    /// Rust project (Cargo.toml)
    Rust,
    /// Node.js/JavaScript project (package.json)
    Node,
    /// Python project (pyproject.toml, setup.py)
    Python,
    /// Go project (go.mod)
    Go,
    /// Java project (pom.xml, build.gradle)
    Java,
    /// Generic project (Makefile, .coderag, etc.)
    Generic,
}

impl ProjectType {
    /// Returns the primary language extensions for this project type.
    ///
    /// This can be used to prioritize certain file types during indexing
    /// or to provide language-specific default configurations.
    pub fn primary_extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::Node => &["ts", "tsx", "js", "jsx"],
            Self::Python => &["py"],
            Self::Go => &["go"],
            Self::Java => &["java"],
            Self::Git | Self::Generic => &[],
        }
    }

    /// Returns a human-readable name for this project type.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Git => "Git Repository",
            Self::Rust => "Rust",
            Self::Node => "Node.js",
            Self::Python => "Python",
            Self::Go => "Go",
            Self::Java => "Java",
            Self::Generic => "Generic",
        }
    }
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Default project markers in priority order.
///
/// The `.coderag` directory is checked first (priority 0) to ensure
/// backward compatibility with explicitly initialized projects.
///
/// Markers are ordered by:
/// 1. Priority value (lower = checked first at each directory level)
/// 2. Specificity (language-specific markers before generic ones)
pub const DEFAULT_MARKERS: &[ProjectMarker] = &[
    // Highest priority: existing CodeRAG configuration
    ProjectMarker::new(".coderag", 0, ProjectType::Generic),
    // Version control (common root indicator)
    ProjectMarker::new(".git", 1, ProjectType::Git),
    // Language-specific markers (priority 2)
    ProjectMarker::new("Cargo.toml", 2, ProjectType::Rust),
    ProjectMarker::new("package.json", 2, ProjectType::Node),
    ProjectMarker::new("pyproject.toml", 2, ProjectType::Python),
    ProjectMarker::new("go.mod", 2, ProjectType::Go),
    ProjectMarker::new("pom.xml", 2, ProjectType::Java),
    ProjectMarker::new("build.gradle", 2, ProjectType::Java),
    // Secondary Python markers (priority 3)
    ProjectMarker::new("setup.py", 3, ProjectType::Python),
    ProjectMarker::new("requirements.txt", 3, ProjectType::Python),
    // Generic project markers (priority 10)
    ProjectMarker::new("Makefile", 10, ProjectType::Generic),
    ProjectMarker::new(".editorconfig", 10, ProjectType::Generic),
];

/// Find a marker by name in the default markers list.
pub fn find_marker(name: &str) -> Option<&'static ProjectMarker> {
    DEFAULT_MARKERS.iter().find(|m| m.name == name)
}

/// Get the `.coderag` marker (highest priority).
pub fn coderag_marker() -> &'static ProjectMarker {
    &DEFAULT_MARKERS[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_type_extensions() {
        assert_eq!(ProjectType::Rust.primary_extensions(), &["rs"]);
        assert_eq!(
            ProjectType::Node.primary_extensions(),
            &["ts", "tsx", "js", "jsx"]
        );
        assert_eq!(ProjectType::Python.primary_extensions(), &["py"]);
        assert_eq!(ProjectType::Go.primary_extensions(), &["go"]);
        assert_eq!(ProjectType::Java.primary_extensions(), &["java"]);
        assert!(ProjectType::Git.primary_extensions().is_empty());
        assert!(ProjectType::Generic.primary_extensions().is_empty());
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(ProjectType::Rust.display_name(), "Rust");
        assert_eq!(ProjectType::Node.display_name(), "Node.js");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
    }

    #[test]
    fn test_default_markers_priority_order() {
        // .coderag should be first (highest priority)
        assert_eq!(DEFAULT_MARKERS[0].name, ".coderag");
        assert_eq!(DEFAULT_MARKERS[0].priority, 0);

        // .git should be second
        assert_eq!(DEFAULT_MARKERS[1].name, ".git");
        assert_eq!(DEFAULT_MARKERS[1].priority, 1);
    }

    #[test]
    fn test_coderag_marker_highest_priority() {
        let coderag = coderag_marker();
        assert_eq!(coderag.name, ".coderag");
        assert_eq!(coderag.priority, 0);

        // Verify no other marker has lower priority
        for marker in DEFAULT_MARKERS.iter() {
            assert!(
                marker.priority >= coderag.priority,
                "Marker {} has higher priority than .coderag",
                marker.name
            );
        }
    }

    #[test]
    fn test_find_marker() {
        let cargo = find_marker("Cargo.toml");
        assert!(cargo.is_some());
        assert_eq!(cargo.unwrap().project_type, ProjectType::Rust);

        let missing = find_marker("nonexistent.file");
        assert!(missing.is_none());
    }

    #[test]
    fn test_marker_equality() {
        let marker1 = ProjectMarker::new("test", 1, ProjectType::Generic);
        let marker2 = ProjectMarker::new("test", 1, ProjectType::Generic);
        let marker3 = ProjectMarker::new("test", 2, ProjectType::Generic);

        assert_eq!(marker1, marker2);
        assert_ne!(marker1, marker3);
    }
}
