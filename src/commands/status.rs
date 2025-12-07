//! Status command implementation.
//!
//! Shows project and index status for the current directory.
//! Useful for debugging and understanding CodeRAG's state.

use anyhow::Result;
use std::env;

use crate::auto_index::StorageResolver;
use crate::project_detection::ProjectDetector;
use crate::storage::Storage;

/// Run the status command.
///
/// Shows information about:
/// - Detected project root and type
/// - Storage location (local vs global)
/// - Index existence and statistics
pub async fn run() -> Result<()> {
    let cwd = env::current_dir()?;
    let detector = ProjectDetector::default();

    match detector.detect(&cwd) {
        Ok(project) => {
            println!("Project root: {}", project.root.display());
            println!("Project type: {:?}", project.project_type);
            println!("Marker: {}", project.marker.name);
            println!("Has local config: {}", project.has_local_config);

            match StorageResolver::resolve(&project) {
                Ok(storage) => {
                    println!();
                    println!(
                        "Storage: {}",
                        if storage.is_local() {
                            "local (.coderag/)"
                        } else {
                            "global (~/.local/share/coderag/)"
                        }
                    );
                    println!("Index path: {}", storage.db_path().display());
                    println!("Index exists: {}", storage.index_exists());

                    // If index exists, show statistics
                    if storage.index_exists() {
                        // Use default dimension since we're only reading metadata
                        match Storage::new_with_default_dimension(storage.db_path()).await {
                            Ok(db) => {
                                let chunk_count = db.count_chunks().await.unwrap_or(0);
                                let files = db.list_files(None).await.unwrap_or_default();

                                println!();
                                println!("Index statistics:");
                                println!("  Files indexed: {}", files.len());
                                println!("  Total chunks: {}", chunk_count);
                            }
                            Err(e) => {
                                println!();
                                println!("Could not read index statistics: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    println!();
                    println!("Storage resolution failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("No project detected: {}", e);
            println!();
            println!("CodeRAG detects projects by looking for markers like:");
            println!("  .git, Cargo.toml, package.json, pyproject.toml, go.mod, etc.");
            println!();
            println!("Run 'coderag init' to create a local .coderag/ directory,");
            println!("or navigate to a directory within a project.");
        }
    }

    Ok(())
}
