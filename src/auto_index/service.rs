//! Auto-index service for zero-ceremony code indexing.
//!
//! This service provides automatic project detection and indexing, allowing
//! users to run `coderag search "query"` from any project directory without
//! explicit initialization.

use std::path::Path;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::indexer::Walker;
use crate::indexing::ParallelIndexer;
use crate::project_detection::{DetectedProject, DetectionError, ProjectDetector};
use crate::search::bm25::Bm25Search;
use crate::storage::Storage;

use super::storage_resolver::{StorageError, StorageLocation, StorageResolver};

/// Errors during auto-indexing.
#[derive(Error, Debug)]
pub enum AutoIndexError {
    /// Project detection failed.
    #[error("Project detection failed: {0}")]
    Detection(#[from] DetectionError),

    /// Storage resolution failed.
    #[error("Storage resolution failed: {0}")]
    Storage(#[from] StorageError),

    /// Indexing operation failed.
    #[error("Indexing failed: {0}")]
    Indexing(#[from] anyhow::Error),

    /// Configuration loading failed.
    #[error("Config loading failed: {0}")]
    Config(String),
}

/// Result of an auto-index operation.
#[derive(Debug, Clone)]
pub struct AutoIndexResult {
    /// Storage location used for the index.
    pub storage: StorageLocation,
    /// Number of files indexed.
    pub files_indexed: usize,
    /// Number of chunks created.
    pub chunks_created: usize,
    /// Whether this was an incremental update (vs fresh index).
    pub was_incremental: bool,
    /// Time taken for indexing in seconds.
    pub duration_secs: f64,
}

impl AutoIndexResult {
    /// Create a result indicating no indexing was performed.
    fn no_indexing(storage: StorageLocation) -> Self {
        Self {
            storage,
            files_indexed: 0,
            chunks_created: 0,
            was_incremental: false,
            duration_secs: 0.0,
        }
    }
}

/// Policy for when to auto-index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoIndexPolicy {
    /// Never auto-index (require explicit `coderag index`).
    Never,
    /// Auto-index only if no index exists.
    OnMissing,
    /// Auto-index if index is missing or stale (files changed).
    OnMissingOrStale,
}

impl Default for AutoIndexPolicy {
    fn default() -> Self {
        Self::OnMissingOrStale
    }
}

/// Service for automatic project detection and indexing.
///
/// This is the main entry point for the zero-ceremony auto-indexing feature.
/// It handles:
/// - Project root detection
/// - Storage location resolution (local vs global)
/// - Configuration loading with hierarchy
/// - Policy-based indexing decisions
/// - Parallel indexing execution
pub struct AutoIndexService {
    /// Project detector instance.
    detector: ProjectDetector,
    /// Policy for when to perform auto-indexing.
    policy: AutoIndexPolicy,
}

impl AutoIndexService {
    /// Create a new auto-index service with default settings.
    ///
    /// Uses `AutoIndexPolicy::OnMissingOrStale` by default.
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
    /// 2. Resolves storage location (local vs global)
    /// 3. Loads configuration (local if exists, else defaults)
    /// 4. Checks if indexing is needed based on policy
    /// 5. Performs indexing if required
    ///
    /// # Arguments
    ///
    /// * `cwd` - The current working directory to start project detection from.
    ///
    /// # Returns
    ///
    /// Returns the storage location and indexing statistics.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let service = AutoIndexService::new();
    /// let result = service.ensure_indexed(&std::env::current_dir()?).await?;
    ///
    /// if result.files_indexed > 0 {
    ///     println!("Indexed {} files", result.files_indexed);
    /// }
    /// ```
    pub async fn ensure_indexed(&self, cwd: &Path) -> Result<AutoIndexResult, AutoIndexError> {
        // Step 1: Detect project root
        let project = self.detector.detect(cwd)?;
        info!("Detected project root: {:?}", project.root);

        // Step 2: Resolve storage location
        let storage = StorageResolver::resolve(&project)?;
        debug!(
            "Storage location: {} ({})",
            storage.db_path().display(),
            if storage.is_local() { "local" } else { "global" }
        );

        // Step 3: Load config (local overrides global)
        let config = self.load_config(&project)?;

        // Step 4: Check if indexing is needed
        let needs_indexing = self.needs_indexing(&storage, &project, &config).await?;

        if !needs_indexing {
            debug!("Index is up to date, skipping indexing");
            return Ok(AutoIndexResult::no_indexing(storage));
        }

        // Step 5: Perform indexing
        info!("Starting auto-indexing...");
        let result = self.perform_indexing(&storage, &project, &config).await?;

        Ok(result)
    }

    /// Get storage location without indexing.
    ///
    /// Useful when you just need to know where the index is/would be stored
    /// without triggering any indexing operations.
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
            AutoIndexPolicy::Never => {
                debug!("Policy is Never, skipping indexing check");
                Ok(false)
            }
            AutoIndexPolicy::OnMissing => {
                let exists = storage.index_exists();
                debug!("Policy is OnMissing, index exists: {}", exists);
                Ok(!exists)
            }
            AutoIndexPolicy::OnMissingOrStale => {
                if !storage.index_exists() {
                    debug!("Index does not exist, needs indexing");
                    return Ok(true);
                }

                // TODO: Implement mtime-based staleness check
                // For now, if index exists, we assume it's up to date.
                // The parallel indexer will still do incremental updates
                // based on file mtimes when indexing is performed.
                debug!("Index exists, assuming up to date (mtime check not yet implemented)");
                Ok(false)
            }
        }
    }

    /// Load configuration with hierarchy: local `.coderag/config.toml` > defaults.
    fn load_config(&self, project: &DetectedProject) -> Result<Config, AutoIndexError> {
        if project.has_local_config {
            // Try to load local config
            Config::load(&project.root).map_err(|e| AutoIndexError::Config(e.to_string()))
        } else {
            // Use defaults for projects without local config
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
            std::fs::create_dir_all(parent).map_err(|e| {
                AutoIndexError::Indexing(anyhow::anyhow!(
                    "Failed to create storage directory: {}",
                    e
                ))
            })?;
        }

        // Create storage and check for existing index
        let db = Storage::new(storage.db_path()).await?;
        let existing_mtimes = db.get_file_mtimes().await?;
        let was_incremental = !existing_mtimes.is_empty();

        if was_incremental {
            debug!(
                "Performing incremental indexing ({} files in existing index)",
                existing_mtimes.len()
            );
        } else {
            debug!("Performing fresh indexing");
        }

        // Collect files to index
        let walker = Walker::new(project.root.clone(), &config.indexer);
        let files: Vec<_> = walker.collect_files();

        if files.is_empty() {
            info!("No files found to index");
            return Ok(AutoIndexResult {
                storage: storage.clone(),
                files_indexed: 0,
                chunks_created: 0,
                was_incremental,
                duration_secs: start.elapsed().as_secs_f64(),
            });
        }

        info!("Found {} files to check", files.len());

        // Use parallel indexer with resolved storage path
        let indexer = ParallelIndexer::with_storage_path(
            project.root.clone(),
            config.clone(),
            Some(storage.db_path().to_path_buf()),
        ).await?;
        let result = indexer.index_files(files).await?;

        // Build BM25 index for hybrid search
        if result.chunks_created > 0 {
            debug!("Building BM25 index...");
            if let Err(e) = self.build_bm25_index(&db, storage).await {
                warn!("Failed to build BM25 index: {}", e);
                // Continue without BM25 - vector search will still work
            }
        }

        let duration = start.elapsed();

        info!(
            "Auto-indexing complete: {} files processed, {} chunks created in {:.2}s",
            result.files_processed,
            result.chunks_created,
            duration.as_secs_f64()
        );

        Ok(AutoIndexResult {
            storage: storage.clone(),
            files_indexed: result.files_processed,
            chunks_created: result.chunks_created,
            was_incremental,
            duration_secs: duration.as_secs_f64(),
        })
    }

    /// Build the BM25 index from all chunks in storage.
    async fn build_bm25_index(
        &self,
        storage: &Storage,
        location: &StorageLocation,
    ) -> Result<(), AutoIndexError> {
        let chunks = storage.get_all_chunks().await?;

        if chunks.is_empty() {
            debug!("No chunks to index in BM25");
            return Ok(());
        }

        debug!("Building BM25 index with {} chunks", chunks.len());

        // Determine the directory for BM25 index
        let bm25_dir = location.bm25_path().parent().unwrap_or(location.bm25_path());

        let bm25 = Bm25Search::new(bm25_dir)?;

        {
            let mut index = bm25.index_mut();
            index.clear()?;
            index.add_chunks(&chunks)?;
            index.commit()?;
        }

        debug!("BM25 index built successfully");
        Ok(())
    }
}

impl Default for AutoIndexService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_policy_default() {
        assert_eq!(AutoIndexPolicy::default(), AutoIndexPolicy::OnMissingOrStale);
    }

    #[test]
    fn test_service_creation() {
        let service = AutoIndexService::new();
        assert_eq!(service.policy, AutoIndexPolicy::OnMissingOrStale);

        let service = AutoIndexService::with_policy(AutoIndexPolicy::OnMissing);
        assert_eq!(service.policy, AutoIndexPolicy::OnMissing);

        let service = AutoIndexService::with_policy(AutoIndexPolicy::Never);
        assert_eq!(service.policy, AutoIndexPolicy::Never);
    }

    #[test]
    fn test_resolve_storage_with_git() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let service = AutoIndexService::new();
        let storage = service.resolve_storage(dir.path()).unwrap();

        // Should use global storage since no .coderag
        assert!(!storage.is_local());
    }

    #[test]
    fn test_resolve_storage_with_coderag() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".coderag")).unwrap();

        let service = AutoIndexService::new();
        let storage = service.resolve_storage(dir.path()).unwrap();

        // Should use local storage since .coderag exists
        assert!(storage.is_local());
    }

    #[test]
    fn test_no_project_error() {
        let dir = tempdir().unwrap();
        // No project markers

        // Create a detector with limited depth to avoid finding parent directories
        let service = AutoIndexService::new();
        let result = service.resolve_storage(dir.path());

        assert!(result.is_err());
        assert!(matches!(result, Err(AutoIndexError::Detection(_))));
    }
}
