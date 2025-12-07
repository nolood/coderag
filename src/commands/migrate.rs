//! Migration from local .coderag/ storage to global storage.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use crate::auto_index::compute_project_id;
use crate::registry::GlobalRegistry;

/// Run the migrate command.
///
/// Migrates index files from local `.coderag/` to global storage at
/// `~/.local/share/coderag/indexes/{project-id}/`.
pub async fn run(keep_local: bool, move_files: bool) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let local_dir = cwd.join(".coderag");

    // Check if local storage exists
    if !local_dir.exists() {
        anyhow::bail!(
            "No local .coderag/ directory found.\n\
             This command migrates from local to global storage.\n\
             Current directory: {}",
            cwd.display()
        );
    }

    let local_db = local_dir.join("index.lance");
    let local_bm25 = local_dir.join("bm25");

    // Check if there's actually an index to migrate
    if !local_db.exists() && !local_bm25.exists() {
        anyhow::bail!(
            "No index found in .coderag/\n\
             Nothing to migrate. Run 'coderag index' first to create an index."
        );
    }

    // Compute global storage path
    let project_id = compute_project_id(&cwd)?;
    let global_dir = GlobalRegistry::global_dir()?;
    let global_index_dir = global_dir.join("indexes").join(&project_id);

    println!("Migration plan:");
    println!("  From: {}", local_dir.display());
    println!("  To:   {}", global_index_dir.display());
    println!("  Project ID: {}", project_id);
    println!();

    // Check if global index already exists
    if global_index_dir.exists() {
        anyhow::bail!(
            "Global index already exists at {}\n\
             Remove it first with: rm -rf \"{}\"",
            global_index_dir.display(),
            global_index_dir.display()
        );
    }

    // Create global directory
    fs::create_dir_all(&global_index_dir)
        .with_context(|| format!("Failed to create directory: {}", global_index_dir.display()))?;

    let global_db = global_index_dir.join("index.lance");
    let global_bm25 = global_index_dir.join("bm25");

    // Migrate index.lance
    if local_db.exists() {
        if move_files {
            println!("Moving index.lance...");
            move_dir(&local_db, &global_db)?;
        } else {
            println!("Copying index.lance...");
            copy_dir_recursive(&local_db, &global_db)?;
        }
        info!("Migrated index.lance to {}", global_db.display());
    }

    // Migrate bm25
    if local_bm25.exists() {
        if move_files {
            println!("Moving bm25 index...");
            move_dir(&local_bm25, &global_bm25)?;
        } else {
            println!("Copying bm25 index...");
            copy_dir_recursive(&local_bm25, &global_bm25)?;
        }
        info!("Migrated bm25 to {}", global_bm25.display());
    }

    // Clean up local storage if not keeping
    if !keep_local && !move_files {
        println!("Removing local index files...");

        if local_db.exists() {
            fs::remove_dir_all(&local_db)
                .with_context(|| format!("Failed to remove {}", local_db.display()))?;
        }

        if local_bm25.exists() {
            fs::remove_dir_all(&local_bm25)
                .with_context(|| format!("Failed to remove {}", local_bm25.display()))?;
        }

        // Check if .coderag/ is now empty (except config.toml)
        let remaining: Vec<_> = fs::read_dir(&local_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "config.toml")
            .collect();

        if remaining.is_empty() {
            // Only config.toml remains, ask about keeping it
            println!(
                "Note: .coderag/config.toml kept for local configuration.\n\
                 Remove .coderag/ entirely to use default config."
            );
        }
    }

    println!();
    println!("Migration complete!");
    println!();
    println!("The project will now use global storage.");
    println!("Index location: {}", global_index_dir.display());

    if !keep_local && local_dir.exists() {
        println!();
        println!("To fully switch to global storage, remove .coderag/:");
        println!("  rm -rf .coderag/");
    }

    Ok(())
}

/// Move a directory (rename if on same filesystem, otherwise copy+delete).
fn move_dir(src: &Path, dst: &Path) -> Result<()> {
    // Try rename first (fast, atomic)
    match fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(_) => {
            // Cross-filesystem move: copy then delete
            copy_dir_recursive(src, dst)?;
            fs::remove_dir_all(src)
                .with_context(|| format!("Failed to remove source after copy: {}", src.display()))?;
            Ok(())
        }
    }
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_dir() {
        fs::copy(src, dst).with_context(|| {
            format!("Failed to copy {} to {}", src.display(), dst.display())
        })?;
        return Ok(());
    }

    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create directory: {}", dst.display()))?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_copy_dir_recursive() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();

        // Create source structure
        let src = src_dir.path().join("source");
        fs::create_dir_all(src.join("subdir")).unwrap();
        fs::write(src.join("file1.txt"), "content1").unwrap();
        fs::write(src.join("subdir/file2.txt"), "content2").unwrap();

        // Copy
        let dst = dst_dir.path().join("dest");
        copy_dir_recursive(&src, &dst).unwrap();

        // Verify
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir/file2.txt").exists());
        assert_eq!(fs::read_to_string(dst.join("file1.txt")).unwrap(), "content1");
        assert_eq!(
            fs::read_to_string(dst.join("subdir/file2.txt")).unwrap(),
            "content2"
        );
    }

    #[test]
    fn test_move_dir_same_filesystem() {
        let temp = tempdir().unwrap();

        // Create source
        let src = temp.path().join("source");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("file.txt"), "content").unwrap();

        // Move
        let dst = temp.path().join("dest");
        move_dir(&src, &dst).unwrap();

        // Source should be gone, dest should exist
        assert!(!src.exists());
        assert!(dst.join("file.txt").exists());
    }
}
