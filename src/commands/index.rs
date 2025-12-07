//! Index command implementation.
//!
//! Indexes the current project using AutoIndexService for zero-ceremony usage.
//! Supports both local (.coderag/) and global (~/.local/share/coderag/) storage.

use anyhow::Result;
use std::env;

use crate::auto_index::{AutoIndexPolicy, AutoIndexService, StorageResolver};
use crate::project_detection::ProjectDetector;
use crate::storage::Storage;

/// Run the index command.
///
/// Uses AutoIndexService for consistent storage resolution.
/// Indexes are stored either locally (if .coderag/ exists) or globally.
///
/// # Arguments
///
/// * `force` - Force full re-index by clearing existing index first
pub async fn run(force: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    // If force flag is set, clear existing index first
    if force {
        let detector = ProjectDetector::default();
        if let Ok(project) = detector.detect(&cwd) {
            if let Ok(storage_location) = StorageResolver::resolve(&project) {
                if storage_location.index_exists() {
                    eprintln!("Clearing existing index for full re-index...");
                    // Use default dimension since we're just clearing
                    let storage = Storage::new_with_default_dimension(storage_location.db_path()).await?;
                    storage.clear().await?;
                }
            }
        }
    }

    // Use AutoIndexService for consistent storage resolution
    // When force is true, we use OnMissing policy since we just cleared the index
    let policy = if force {
        AutoIndexPolicy::OnMissing
    } else {
        AutoIndexPolicy::OnMissingOrStale
    };
    let service = AutoIndexService::with_policy(policy);
    let result = service.ensure_indexed(&cwd).await?;

    // Print storage location info
    println!("Project root: {}", result.storage.root().display());
    println!(
        "Storage: {}",
        if result.storage.is_local() {
            "local (.coderag/)"
        } else {
            "global (~/.local/share/coderag/)"
        }
    );

    // Print indexing results
    if result.files_indexed > 0 {
        println!(
            "Indexed {} files ({} chunks) in {:.2}s",
            result.files_indexed, result.chunks_created, result.duration_secs
        );
        if result.was_incremental {
            println!("(incremental update)");
        }
    } else {
        println!("Index is up to date. No files need indexing.");
    }

    Ok(())
}
