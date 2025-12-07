# Zero-Ceremony Auto-Indexing Architecture

## Executive Summary

This document defines the architecture for CodeRAG's "zero-ceremony auto-indexing" feature, enabling users to run `coderag search "query"` from any project directory without explicit initialization. The system auto-detects project roots, stores indexes globally, and maintains backward compatibility with existing `.coderag/` configurations.

---

## 1. Module Structure

### 1.1 New Modules

```
src/
  project_detection/
    mod.rs              # Module exports
    detector.rs         # Project root detection logic
    markers.rs          # Project marker definitions

  auto_index/
    mod.rs              # Module exports
    service.rs          # AutoIndexService implementation
    storage_resolver.rs # Global vs local storage resolution

  config/
    mod.rs              # Refactored config module
    loader.rs           # Hierarchical config loading
    merged.rs           # Config merging logic
```

### 1.2 Modified Modules

| Module | Changes |
|--------|---------|
| `src/config.rs` | Split into `src/config/` module, add global config support |
| `src/registry/global.rs` | Add `indexes/` directory management |
| `src/commands/search.rs` | Integrate `AutoIndexService` for auto-indexing |
| `src/commands/index.rs` | Support global storage location |
| `src/storage/lancedb.rs` | Accept dynamic storage paths |

### 1.3 Dependency Graph

```
commands/search.rs
       |
       v
AutoIndexService (new)
       |
       +---> ProjectDetector (new)
       |           |
       |           v
       |     ProjectMarkers (new)
       |
       +---> StorageResolver (new)
       |           |
       |           v
       |     GlobalRegistry (modified)
       |
       +---> ConfigLoader (new)
       |           |
       |           v
       |     MergedConfig (new)
       |
       v
Storage / Indexer (existing)
```

---

## 2. Data Structures

### 2.1 Project Detection

```rust
// src/project_detection/markers.rs

use std::path::Path;

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

/// Recognized project types based on detected markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectType {
    Git,
    Rust,
    Node,
    Python,
    Go,
    Java,
    Generic,
}

impl ProjectType {
    /// Returns the primary language extensions for this project type.
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
}

/// Default project markers in priority order.
pub const DEFAULT_MARKERS: &[ProjectMarker] = &[
    ProjectMarker { name: ".git", priority: 0, project_type: ProjectType::Git },
    ProjectMarker { name: "Cargo.toml", priority: 1, project_type: ProjectType::Rust },
    ProjectMarker { name: "package.json", priority: 1, project_type: ProjectType::Node },
    ProjectMarker { name: "pyproject.toml", priority: 1, project_type: ProjectType::Python },
    ProjectMarker { name: "setup.py", priority: 2, project_type: ProjectType::Python },
    ProjectMarker { name: "go.mod", priority: 1, project_type: ProjectType::Go },
    ProjectMarker { name: "pom.xml", priority: 1, project_type: ProjectType::Java },
    ProjectMarker { name: "build.gradle", priority: 1, project_type: ProjectType::Java },
    ProjectMarker { name: "Makefile", priority: 10, project_type: ProjectType::Generic },
    ProjectMarker { name: ".coderag", priority: 0, project_type: ProjectType::Generic },
];
```

```rust
// src/project_detection/detector.rs

use std::path::{Path, PathBuf};
use thiserror::Error;

use super::markers::{ProjectMarker, ProjectType, DEFAULT_MARKERS};

/// Errors that can occur during project detection.
#[derive(Error, Debug)]
pub enum DetectionError {
    #[error("No project root found from {starting_dir}")]
    NoProjectRoot { starting_dir: PathBuf },

    #[error("IO error during detection: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path canonicalization failed: {path}")]
    Canonicalization { path: PathBuf },
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

/// Detects project roots by traversing up the directory tree.
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
            max_depth: 100,
        }
    }
}

impl ProjectDetector {
    /// Create a new detector with custom settings.
    pub fn new(markers: &'static [ProjectMarker], max_depth: usize) -> Self {
        Self { markers, max_depth }
    }

    /// Detect project root starting from the given directory.
    ///
    /// Traverses upward until a project marker is found or root is reached.
    pub fn detect(&self, starting_dir: &Path) -> Result<DetectedProject, DetectionError> {
        let canonical = starting_dir
            .canonicalize()
            .map_err(|_| DetectionError::Canonicalization {
                path: starting_dir.to_path_buf(),
            })?;

        let mut current = canonical.as_path();
        let mut depth = 0;

        while depth < self.max_depth {
            // Check for .coderag first (explicit initialization)
            let coderag_dir = current.join(".coderag");
            if coderag_dir.is_dir() {
                return Ok(DetectedProject {
                    root: current.to_path_buf(),
                    marker: &DEFAULT_MARKERS[9], // .coderag marker
                    project_type: ProjectType::Generic,
                    has_local_config: true,
                });
            }

            // Check other markers by priority
            for marker in self.markers.iter().filter(|m| m.name != ".coderag") {
                let marker_path = current.join(marker.name);
                if marker_path.exists() {
                    return Ok(DetectedProject {
                        root: current.to_path_buf(),
                        marker,
                        project_type: marker.project_type,
                        has_local_config: false,
                    });
                }
            }

            // Move to parent directory
            match current.parent() {
                Some(parent) if parent != current => {
                    current = parent;
                    depth += 1;
                }
                _ => break,
            }
        }

        Err(DetectionError::NoProjectRoot {
            starting_dir: starting_dir.to_path_buf(),
        })
    }

    /// Check if a directory is a project root.
    pub fn is_project_root(&self, dir: &Path) -> bool {
        self.markers.iter().any(|m| dir.join(m.name).exists())
    }
}
```

### 2.2 Storage Resolution

```rust
// src/auto_index/storage_resolver.rs

use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::registry::GlobalRegistry;
use crate::project_detection::DetectedProject;

/// Errors during storage resolution.
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Failed to compute project ID: {reason}")]
    ProjectIdError { reason: String },

    #[error("Failed to access global directory: {0}")]
    GlobalDirError(#[from] anyhow::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Where the index for a project is stored.
#[derive(Debug, Clone)]
pub enum StorageLocation {
    /// Local storage in .coderag/ directory (existing projects)
    Local {
        root: PathBuf,
        db_path: PathBuf,
        bm25_path: PathBuf,
    },
    /// Global storage in ~/.local/share/coderag/indexes/{project_id}/
    Global {
        root: PathBuf,
        project_id: String,
        db_path: PathBuf,
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
}

/// Resolves where to store/find indexes for a project.
pub struct StorageResolver;

impl StorageResolver {
    /// Resolve storage location for a detected project.
    ///
    /// Priority:
    /// 1. If .coderag/ exists locally, use local storage (backward compat)
    /// 2. Otherwise, use global storage
    pub fn resolve(project: &DetectedProject) -> Result<StorageLocation, StorageError> {
        if project.has_local_config {
            Self::resolve_local(&project.root)
        } else {
            Self::resolve_global(&project.root)
        }
    }

    /// Resolve to local .coderag/ storage.
    fn resolve_local(root: &Path) -> Result<StorageLocation, StorageError> {
        let coderag_dir = root.join(".coderag");
        Ok(StorageLocation::Local {
            root: root.to_path_buf(),
            db_path: coderag_dir.join("index.lance"),
            bm25_path: coderag_dir.join("bm25"),
        })
    }

    /// Resolve to global ~/.local/share/coderag/indexes/ storage.
    fn resolve_global(root: &Path) -> Result<StorageLocation, StorageError> {
        let project_id = Self::compute_project_id(root)?;
        let global_dir = GlobalRegistry::global_dir()?;
        let index_dir = global_dir.join("indexes").join(&project_id);

        Ok(StorageLocation::Global {
            root: root.to_path_buf(),
            project_id,
            db_path: index_dir.join("index.lance"),
            bm25_path: index_dir.join("bm25"),
        })
    }

    /// Compute a stable, unique project ID from the root path.
    ///
    /// Uses a combination of:
    /// - Directory name (for readability)
    /// - Hash of canonical path (for uniqueness)
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

        // Combine: "project-name-abc12345"
        Ok(format!("{}-{:08x}", sanitize_name(dir_name), hash as u32))
    }
}

/// Sanitize a directory name for use in file paths.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .to_lowercase()
}
```

### 2.3 Auto-Index Service

```rust
// src/auto_index/service.rs

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::indexer::Walker;
use crate::indexing::ParallelIndexer;
use crate::project_detection::{DetectedProject, ProjectDetector};
use crate::storage::Storage;

use super::storage_resolver::{StorageLocation, StorageResolver};

/// Errors during auto-indexing.
#[derive(Error, Debug)]
pub enum AutoIndexError {
    #[error("Project detection failed: {0}")]
    Detection(#[from] crate::project_detection::DetectionError),

    #[error("Storage resolution failed: {0}")]
    Storage(#[from] super::storage_resolver::StorageError),

    #[error("Indexing failed: {0}")]
    Indexing(#[from] anyhow::Error),

    #[error("Config loading failed: {0}")]
    Config(String),
}

/// Result of an auto-index operation.
#[derive(Debug)]
pub struct AutoIndexResult {
    /// Storage location used
    pub storage: StorageLocation,
    /// Number of files indexed
    pub files_indexed: usize,
    /// Number of chunks created
    pub chunks_created: usize,
    /// Whether this was a fresh index or incremental update
    pub was_incremental: bool,
    /// Time taken for indexing
    pub duration_secs: f64,
}

/// Policy for when to auto-index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoIndexPolicy {
    /// Never auto-index (require explicit `coderag index`)
    Never,
    /// Auto-index only if no index exists
    OnMissing,
    /// Auto-index if index is missing or stale (files changed)
    OnMissingOrStale,
}

impl Default for AutoIndexPolicy {
    fn default() -> Self {
        Self::OnMissingOrStale
    }
}

/// Service for automatic project detection and indexing.
pub struct AutoIndexService {
    detector: ProjectDetector,
    policy: AutoIndexPolicy,
}

impl AutoIndexService {
    /// Create a new auto-index service with default settings.
    pub fn new() -> Self {
        Self {
            detector: ProjectDetector::default(),
            policy: AutoIndexPolicy::default(),
        }
    }

    /// Create with a specific policy.
    pub fn with_policy(policy: AutoIndexPolicy) -> Self {
        Self {
            detector: ProjectDetector::default(),
            policy,
        }
    }

    /// Ensure an index exists for the project containing `cwd`.
    ///
    /// This is the main entry point for auto-indexing. It:
    /// 1. Detects the project root
    /// 2. Resolves storage location
    /// 3. Checks if indexing is needed
    /// 4. Performs indexing if required
    ///
    /// Returns the storage location and index stats.
    pub async fn ensure_indexed(&self, cwd: &Path) -> Result<AutoIndexResult, AutoIndexError> {
        // Step 1: Detect project root
        let project = self.detector.detect(cwd)?;
        info!("Detected project root: {:?}", project.root);

        // Step 2: Resolve storage location
        let storage = StorageResolver::resolve(&project)?;
        debug!("Storage location: {:?}", storage);

        // Step 3: Load config (local overrides global)
        let config = self.load_config(&project)?;

        // Step 4: Check if indexing is needed
        let needs_indexing = self.needs_indexing(&storage, &project, &config).await?;

        if !needs_indexing {
            debug!("Index is up to date, skipping");
            return Ok(AutoIndexResult {
                storage,
                files_indexed: 0,
                chunks_created: 0,
                was_incremental: false,
                duration_secs: 0.0,
            });
        }

        // Step 5: Perform indexing
        let result = self.perform_indexing(&storage, &project, &config).await?;

        Ok(result)
    }

    /// Get storage location without indexing.
    pub fn resolve_storage(&self, cwd: &Path) -> Result<StorageLocation, AutoIndexError> {
        let project = self.detector.detect(cwd)?;
        let storage = StorageResolver::resolve(&project)?;
        Ok(storage)
    }

    /// Check if indexing is needed based on policy.
    async fn needs_indexing(
        &self,
        storage: &StorageLocation,
        _project: &DetectedProject,
        _config: &Config,
    ) -> Result<bool, AutoIndexError> {
        match self.policy {
            AutoIndexPolicy::Never => Ok(false),
            AutoIndexPolicy::OnMissing => Ok(!storage.index_exists()),
            AutoIndexPolicy::OnMissingOrStale => {
                if !storage.index_exists() {
                    return Ok(true);
                }

                // TODO: Check for stale files using mtime comparison
                // For now, just check existence
                Ok(false)
            }
        }
    }

    /// Load configuration with hierarchy: local .coderag/config.toml > global > defaults.
    fn load_config(&self, project: &DetectedProject) -> Result<Config, AutoIndexError> {
        // Try local config first
        if project.has_local_config {
            Config::load(&project.root)
                .map_err(|e| AutoIndexError::Config(e.to_string()))
        } else {
            // Use defaults (could add global config support later)
            Ok(Config::default())
        }
    }

    /// Perform the actual indexing operation.
    async fn perform_indexing(
        &self,
        storage: &StorageLocation,
        project: &DetectedProject,
        config: &Config,
    ) -> Result<AutoIndexResult, AutoIndexError> {
        let start = Instant::now();

        // Ensure storage directory exists
        if let Some(parent) = storage.db_path().parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create storage and indexer
        let db = Storage::new(storage.db_path()).await?;
        let walker = Walker::new(project.root.clone(), &config.indexer);
        let files: Vec<_> = walker.collect_files();

        info!("Found {} files to index", files.len());

        // Check for incremental update
        let existing_mtimes = db.get_file_mtimes().await?;
        let was_incremental = !existing_mtimes.is_empty();

        // Use parallel indexer if configured
        let indexer = ParallelIndexer::new(project.root.clone(), config.clone()).await?;
        let result = indexer.index_files(files).await?;

        let duration = start.elapsed();

        Ok(AutoIndexResult {
            storage: storage.clone(),
            files_indexed: result.files_processed,
            chunks_created: result.chunks_created,
            was_incremental,
            duration_secs: duration.as_secs_f64(),
        })
    }
}

impl Default for AutoIndexService {
    fn default() -> Self {
        Self::new()
    }
}
```

---

## 3. API Design

### 3.1 Public API Surface

```rust
// Primary entry points for the zero-ceremony feature

/// Project detection
pub use project_detection::{
    ProjectDetector,     // Main detector
    DetectedProject,     // Detection result
    DetectionError,      // Error type
    ProjectType,         // Project type enum
    ProjectMarker,       // Marker definition
};

/// Storage resolution
pub use auto_index::{
    StorageResolver,     // Resolves storage location
    StorageLocation,     // Enum: Local/Global
    StorageError,        // Error type
};

/// Auto-indexing service
pub use auto_index::{
    AutoIndexService,    // Main service
    AutoIndexResult,     // Indexing result
    AutoIndexPolicy,     // When to auto-index
    AutoIndexError,      // Error type
};
```

### 3.2 Key Function Signatures

```rust
// Project detection
impl ProjectDetector {
    /// Detect project root from a starting directory.
    pub fn detect(&self, starting_dir: &Path) -> Result<DetectedProject, DetectionError>;

    /// Check if a directory is a project root.
    pub fn is_project_root(&self, dir: &Path) -> bool;
}

// Storage resolution
impl StorageResolver {
    /// Resolve storage location for a detected project.
    pub fn resolve(project: &DetectedProject) -> Result<StorageLocation, StorageError>;

    /// Compute a unique project ID from path.
    pub fn compute_project_id(root: &Path) -> Result<String, StorageError>;
}

// Auto-index service
impl AutoIndexService {
    /// Ensure index exists, creating if needed.
    pub async fn ensure_indexed(&self, cwd: &Path) -> Result<AutoIndexResult, AutoIndexError>;

    /// Get storage location without indexing.
    pub fn resolve_storage(&self, cwd: &Path) -> Result<StorageLocation, AutoIndexError>;
}
```

---

## 4. Integration Points

### 4.1 Search Command Integration

```rust
// src/commands/search.rs (modified)

use crate::auto_index::{AutoIndexService, AutoIndexPolicy};

/// Run the search command with auto-indexing.
pub async fn run(query: &str, limit: Option<usize>) -> Result<()> {
    let cwd = env::current_dir()?;

    // Auto-index service handles:
    // 1. Project detection
    // 2. Storage resolution (local vs global)
    // 3. Auto-indexing if needed
    let service = AutoIndexService::with_policy(AutoIndexPolicy::OnMissingOrStale);

    let result = service.ensure_indexed(&cwd).await?;

    if result.files_indexed > 0 {
        println!(
            "Auto-indexed {} files ({} chunks) in {:.2}s",
            result.files_indexed,
            result.chunks_created,
            result.duration_secs
        );
    }

    // Use resolved storage location
    let config = Config::load(result.storage.root())?;
    let limit = limit.unwrap_or(config.search.default_limit);

    // Initialize components with resolved paths
    let storage = Arc::new(Storage::new(result.storage.db_path()).await?);
    let embedder = Arc::new(EmbeddingGenerator::new(&config.embeddings)?);
    let search_engine = SearchEngine::new(storage, embedder);

    // Perform search...
    let results = search_engine.search(query, limit).await?;
    // ... rest of search logic
}
```

### 4.2 Index Command Integration

```rust
// src/commands/index.rs (modified)

use crate::auto_index::{AutoIndexService, StorageResolver};
use crate::project_detection::ProjectDetector;

/// Run the index command.
///
/// Supports both local .coderag/ and global storage.
pub async fn run() -> Result<()> {
    let cwd = env::current_dir()?;

    // Detect project and resolve storage
    let detector = ProjectDetector::default();
    let project = detector.detect(&cwd)?;
    let storage = StorageResolver::resolve(&project)?;

    println!("Project root: {}", project.root.display());
    println!(
        "Index location: {}",
        if storage.is_local() { "local (.coderag/)" } else { "global" }
    );

    // Continue with existing indexing logic using resolved storage path...
}
```

### 4.3 Init Command Integration

```rust
// src/commands/init.rs (modified)

/// Run init command - creates local .coderag/ for explicit configuration.
///
/// Note: init is now optional. Projects can be indexed globally without init.
pub async fn run(force_local: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if Config::is_initialized(&cwd) && !force_local {
        println!("CodeRAG is already initialized locally.");
        println!("Use --force to reinitialize.");
        return Ok(());
    }

    // Create local config
    let config = Config::default();
    config.save(&cwd)?;

    println!("Created .coderag/config.toml");
    println!("\nThis project will now use local storage instead of global.");
}
```

### 4.4 Registry Integration

```rust
// src/registry/global.rs (additions)

impl GlobalRegistry {
    /// Get the global indexes directory.
    pub fn indexes_dir() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join("indexes"))
    }

    /// List all globally indexed projects.
    pub fn list_global_indexes() -> Result<Vec<GlobalIndexInfo>> {
        let indexes_dir = Self::indexes_dir()?;

        if !indexes_dir.exists() {
            return Ok(Vec::new());
        }

        let mut indexes = Vec::new();

        for entry in std::fs::read_dir(&indexes_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                let project_id = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let db_path = path.join("index.lance");
                let has_index = db_path.exists();

                indexes.push(GlobalIndexInfo {
                    project_id,
                    index_path: path,
                    has_index,
                });
            }
        }

        Ok(indexes)
    }

    /// Remove a global index by project ID.
    pub fn remove_global_index(project_id: &str) -> Result<()> {
        let index_path = Self::indexes_dir()?.join(project_id);

        if index_path.exists() {
            std::fs::remove_dir_all(&index_path)?;
            info!("Removed global index: {}", project_id);
        }

        Ok(())
    }
}

/// Information about a globally stored index.
#[derive(Debug, Clone)]
pub struct GlobalIndexInfo {
    pub project_id: String,
    pub index_path: PathBuf,
    pub has_index: bool,
}
```

---

## 5. Storage Layout

### 5.1 Global Storage Structure

```
~/.local/share/coderag/
  registry.json                    # Project registry (existing)
  indexes/                         # NEW: Global index storage
    {project-id}/                  # Per-project directory
      index.lance/                 # LanceDB vector database
      bm25/                        # Tantivy BM25 index
      metadata.json                # Project metadata cache
```

### 5.2 Project ID Format

```
{sanitized-name}-{path-hash}

Examples:
  coderag-a1b2c3d4           # /home/user/projects/coderag
  my-app-e5f6g7h8            # /home/user/work/my-app
  rust-project-12345678      # /var/projects/rust-project
```

### 5.3 Metadata Cache

```rust
// ~/.local/share/coderag/indexes/{project-id}/metadata.json

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexMetadata {
    /// Original project root path
    pub project_root: PathBuf,
    /// Project ID used for this index
    pub project_id: String,
    /// When the index was created
    pub created_at: DateTime<Utc>,
    /// When the index was last updated
    pub last_updated: DateTime<Utc>,
    /// Number of files in the index
    pub file_count: usize,
    /// Number of chunks in the index
    pub chunk_count: usize,
    /// Config hash for detecting config changes
    pub config_hash: String,
}
```

### 5.4 Local Storage (Backward Compat)

```
{project-root}/
  .coderag/
    config.toml                    # Local configuration
    index.lance/                   # LanceDB database
    bm25/                          # BM25 index
```

---

## 6. Configuration Hierarchy

### 6.1 Config Loading Order

```
1. Local:  {project}/.coderag/config.toml  (highest priority)
2. Global: ~/.config/coderag/config.toml   (future: global defaults)
3. Defaults: Compiled-in defaults          (lowest priority)
```

### 6.2 Config Merging Logic

```rust
// src/config/merged.rs

use crate::Config;

/// Merge two configs, with `override_config` taking precedence.
pub fn merge_configs(base: Config, override_config: Config) -> Config {
    Config {
        indexer: merge_indexer(base.indexer, override_config.indexer),
        embeddings: merge_embeddings(base.embeddings, override_config.embeddings),
        storage: merge_storage(base.storage, override_config.storage),
        server: merge_server(base.server, override_config.server),
        search: merge_search(base.search, override_config.search),
    }
}

/// Load config with full hierarchy support.
pub fn load_merged_config(project_root: &Path) -> Result<Config> {
    let mut config = Config::default();

    // TODO: Load global config from ~/.config/coderag/config.toml
    // if let Some(global) = load_global_config()? {
    //     config = merge_configs(config, global);
    // }

    // Load local config (overrides global)
    let local_path = project_root.join(".coderag/config.toml");
    if local_path.exists() {
        let local = Config::load(project_root)?;
        config = merge_configs(config, local);
    }

    Ok(config)
}
```

### 6.3 Per-Project Overrides

```toml
# .coderag/config.toml - Project-specific overrides

[indexer]
# Only index Rust files for this project
extensions = ["rs"]
# Use AST chunking
chunker_strategy = "ast"

[search]
# Prefer vector search for this codebase
mode = "vector"
```

---

## 7. Error Handling

### 7.1 Error Type Hierarchy

```rust
// src/error.rs (new unified error module)

use thiserror::Error;
use std::path::PathBuf;

/// Top-level error type for CodeRAG operations.
#[derive(Error, Debug)]
pub enum CodeRagError {
    #[error("Project detection failed: {0}")]
    Detection(#[from] crate::project_detection::DetectionError),

    #[error("Storage error: {0}")]
    Storage(#[from] crate::auto_index::StorageError),

    #[error("Indexing error: {0}")]
    Indexing(#[from] crate::auto_index::AutoIndexError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Search error: {0}")]
    Search(#[from] anyhow::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for CodeRAG operations.
pub type Result<T> = std::result::Result<T, CodeRagError>;
```

### 7.2 Error Recovery Strategies

| Error | Recovery Strategy |
|-------|-------------------|
| `NoProjectRoot` | Prompt user to run from a project directory or use `--path` |
| `StorageError::ProjectIdError` | Fall back to timestamp-based ID |
| `IndexingError` | Log warning, return empty results, suggest `coderag index` |
| `ConfigError` | Use defaults, log warning |
| `StorageError::GlobalDirError` | Create directory if possible, else fail with clear message |

### 7.3 Graceful Degradation

```rust
// Example: Search with graceful fallback

pub async fn search_with_fallback(query: &str, cwd: &Path) -> Result<Vec<SearchResult>> {
    let service = AutoIndexService::new();

    match service.ensure_indexed(cwd).await {
        Ok(result) => {
            // Normal path: index exists or was created
            perform_search(query, &result.storage).await
        }
        Err(AutoIndexError::Detection(DetectionError::NoProjectRoot { .. })) => {
            // No project found - provide helpful message
            eprintln!("Warning: No project detected. Searching in current directory only.");
            eprintln!("Tip: Run from a directory with .git, Cargo.toml, package.json, etc.");

            // Try to search current directory anyway
            let storage = StorageLocation::Local {
                root: cwd.to_path_buf(),
                db_path: cwd.join(".coderag/index.lance"),
                bm25_path: cwd.join(".coderag/bm25"),
            };

            if storage.index_exists() {
                perform_search(query, &storage).await
            } else {
                Err(CodeRagError::Config(
                    "No index found. Run 'coderag init && coderag index' first.".into()
                ))
            }
        }
        Err(e) => Err(e.into()),
    }
}
```

---

## 8. Migration Path

### 8.1 Backward Compatibility

Existing projects with `.coderag/` directories continue to work unchanged:

1. `ProjectDetector` checks for `.coderag/` first (highest priority marker)
2. `StorageResolver` returns `StorageLocation::Local` when `.coderag/` exists
3. All commands use resolved storage path, working with both local and global

### 8.2 Migration Command (Optional)

```rust
// Future: coderag migrate --to-global

/// Migrate a local .coderag/ index to global storage.
pub async fn migrate_to_global(root: &Path) -> Result<()> {
    let local_db = root.join(".coderag/index.lance");
    let local_bm25 = root.join(".coderag/bm25");

    if !local_db.exists() {
        bail!("No local index found to migrate");
    }

    // Compute global location
    let project_id = StorageResolver::compute_project_id(root)?;
    let global_dir = GlobalRegistry::indexes_dir()?.join(&project_id);

    // Copy index files
    std::fs::create_dir_all(&global_dir)?;
    copy_dir_all(&local_db, global_dir.join("index.lance"))?;
    copy_dir_all(&local_bm25, global_dir.join("bm25"))?;

    // Optionally remove local index (keep config)
    // std::fs::remove_dir_all(&local_db)?;
    // std::fs::remove_dir_all(&local_bm25)?;

    println!("Migrated index to global storage: {}", project_id);
    Ok(())
}
```

---

## 9. Testing Strategy

### 9.1 Unit Tests

```rust
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
    }

    #[test]
    fn test_project_id_stability() {
        let dir = tempdir().unwrap();

        let id1 = StorageResolver::compute_project_id(dir.path()).unwrap();
        let id2 = StorageResolver::compute_project_id(dir.path()).unwrap();

        assert_eq!(id1, id2, "Project ID should be stable");
    }

    #[test]
    fn test_local_storage_priority() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".coderag")).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let detector = ProjectDetector::default();
        let project = detector.detect(dir.path()).unwrap();
        let storage = StorageResolver::resolve(&project).unwrap();

        assert!(storage.is_local(), "Should use local storage when .coderag exists");
    }
}
```

### 9.2 Integration Tests

```rust
#[tokio::test]
async fn test_auto_index_new_project() {
    let dir = tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();

    // Create a Rust file
    std::fs::write(
        dir.path().join("main.rs"),
        "fn main() { println!(\"Hello\"); }"
    ).unwrap();

    let service = AutoIndexService::with_policy(AutoIndexPolicy::OnMissing);
    let result = service.ensure_indexed(dir.path()).await.unwrap();

    assert!(result.files_indexed > 0);
    assert!(result.storage.index_exists());
}
```

---

## 10. Performance Considerations

### 10.1 Lazy Detection

Project detection is fast (single directory traversal), but should still be cached within a session:

```rust
use std::sync::OnceLock;

static CACHED_PROJECT: OnceLock<DetectedProject> = OnceLock::new();

pub fn get_or_detect_project(cwd: &Path) -> Result<&DetectedProject> {
    CACHED_PROJECT.get_or_try_init(|| {
        ProjectDetector::default().detect(cwd)
    })
}
```

### 10.2 Index Staleness Check

Avoid full file system scan by using metadata:

```rust
impl StorageLocation {
    /// Quick check if index might be stale.
    pub fn might_be_stale(&self) -> bool {
        let metadata_path = match self {
            Self::Global { db_path, .. } => db_path.parent().map(|p| p.join("metadata.json")),
            Self::Local { root, .. } => Some(root.join(".coderag/metadata.json")),
        };

        // If no metadata, assume stale
        let Some(path) = metadata_path else { return true };
        let Ok(content) = std::fs::read_to_string(&path) else { return true };
        let Ok(metadata): Result<IndexMetadata, _> = serde_json::from_str(&content) else { return true };

        // Check if last update was more than 1 hour ago
        let age = Utc::now() - metadata.last_updated;
        age.num_hours() > 1
    }
}
```

---

## 11. Future Extensions

### 11.1 Watch Mode Integration

```rust
// Auto-reindex on file changes
impl AutoIndexService {
    pub async fn watch_and_reindex(&self, cwd: &Path) -> Result<()> {
        let storage = self.resolve_storage(cwd)?;
        let watcher = FileWatcher::new(storage.root())?;

        loop {
            let changes = watcher.wait_for_changes().await?;
            if !changes.is_empty() {
                self.ensure_indexed(cwd).await?;
            }
        }
    }
}
```

### 11.2 Multi-Project Search

```rust
// Search across all globally indexed projects
pub async fn search_all_projects(query: &str) -> Result<Vec<(String, Vec<SearchResult>)>> {
    let indexes = GlobalRegistry::list_global_indexes()?;

    let mut results = Vec::new();
    for index in indexes {
        let storage = Storage::new(&index.index_path.join("index.lance")).await?;
        let project_results = storage.search(query, 10).await?;
        results.push((index.project_id, project_results));
    }

    Ok(results)
}
```

### 11.3 Remote Index Sync

```rust
// Sync indexes to/from remote storage (S3, etc.)
pub trait IndexSync {
    async fn push(&self, storage: &StorageLocation) -> Result<()>;
    async fn pull(&self, project_id: &str) -> Result<StorageLocation>;
}
```

---

## 12. Implementation Checklist

### Phase 1: Core Detection (Week 1)
- [ ] Create `src/project_detection/` module
- [ ] Implement `ProjectDetector` with marker-based detection
- [ ] Add unit tests for detection logic
- [ ] Integrate with existing `commands/init.rs`

### Phase 2: Storage Resolution (Week 1-2)
- [ ] Create `src/auto_index/storage_resolver.rs`
- [ ] Implement `StorageLocation` enum
- [ ] Add global index directory management to `GlobalRegistry`
- [ ] Implement stable project ID computation

### Phase 3: Auto-Index Service (Week 2)
- [ ] Create `src/auto_index/service.rs`
- [ ] Implement `AutoIndexService::ensure_indexed()`
- [ ] Add policy-based indexing decisions
- [ ] Integration tests with temp directories

### Phase 4: Command Integration (Week 3)
- [ ] Modify `commands/search.rs` to use `AutoIndexService`
- [ ] Modify `commands/index.rs` to support global storage
- [ ] Update `commands/init.rs` to be optional
- [ ] Add `--local` / `--global` flags to commands

### Phase 5: Polish (Week 3-4)
- [ ] Add progress indicators for auto-indexing
- [ ] Implement config hierarchy merging
- [ ] Add `coderag status` command showing project/storage info
- [ ] Documentation and user guide

---

## Appendix: Full File Manifest

| File | Status | Description |
|------|--------|-------------|
| `src/project_detection/mod.rs` | NEW | Module exports |
| `src/project_detection/detector.rs` | NEW | `ProjectDetector` implementation |
| `src/project_detection/markers.rs` | NEW | Project marker definitions |
| `src/auto_index/mod.rs` | NEW | Module exports |
| `src/auto_index/service.rs` | NEW | `AutoIndexService` implementation |
| `src/auto_index/storage_resolver.rs` | NEW | `StorageResolver` implementation |
| `src/config.rs` | MODIFY | Add global config support |
| `src/registry/global.rs` | MODIFY | Add `indexes_dir()`, `list_global_indexes()` |
| `src/commands/search.rs` | MODIFY | Integrate `AutoIndexService` |
| `src/commands/index.rs` | MODIFY | Support global storage |
| `src/commands/init.rs` | MODIFY | Make optional, add `--force` flag |
| `src/lib.rs` | MODIFY | Export new modules |
