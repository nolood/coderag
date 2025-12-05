//! Project management commands for multi-project support.

use anyhow::{bail, Result};
use std::env;
use std::path::{Path, PathBuf};

use crate::registry::{GlobalRegistry, ProjectInfo, ProjectStats};
use crate::storage::Storage;
use crate::Config;

/// List all registered projects.
pub async fn list() -> Result<()> {
    let registry = GlobalRegistry::load()?;

    if registry.is_empty() {
        println!("No projects registered.");
        println!("\nTo add a project, navigate to its directory and run:");
        println!("  coderag projects add <name>");
        return Ok(());
    }

    let default_name = registry.default_project.as_deref();

    println!("Registered projects:\n");
    println!(
        "{:<3} {:<20} {:<50} {:>10} {:>10}",
        "", "NAME", "PATH", "FILES", "CHUNKS"
    );
    println!("{}", "-".repeat(95));

    for project in registry.list_projects() {
        let is_default = Some(project.name.as_str()) == default_name;
        let default_marker = if is_default { "*" } else { "" };

        let (file_count, chunk_count) = project
            .stats
            .as_ref()
            .map(|s| (s.file_count.to_string(), s.chunk_count.to_string()))
            .unwrap_or_else(|| ("-".to_string(), "-".to_string()));

        let path_display = truncate_path(&project.display_path(), 48);
        let path_status = if project.path_exists() {
            path_display
        } else {
            format!("{} (missing)", path_display)
        };

        println!(
            "{:<3} {:<20} {:<50} {:>10} {:>10}",
            default_marker,
            truncate_string(&project.name, 18),
            path_status,
            file_count,
            chunk_count
        );
    }

    println!("\n* = default project");
    println!("\nTotal: {} project(s)", registry.project_count());

    Ok(())
}

/// Add the current directory as a new project.
pub async fn add(name: String) -> Result<()> {
    let current_dir = env::current_dir()?;

    // Canonicalize the path
    let canonical_path = current_dir
        .canonicalize()
        .unwrap_or_else(|_| current_dir.clone());

    // Check if CodeRAG is initialized in this directory
    if !Config::is_initialized(&canonical_path) {
        bail!(
            "CodeRAG is not initialized in {:?}.\n\
             Run 'coderag init' first to initialize the project.",
            canonical_path
        );
    }

    let mut registry = GlobalRegistry::load()?;

    // Check if a project with this path already exists
    if let Some(existing) = registry.find_by_path(&canonical_path) {
        bail!(
            "This directory is already registered as project '{}'.\n\
             Use 'coderag projects remove {}' first if you want to re-register it.",
            existing.name,
            existing.name
        );
    }

    // Try to get stats from the existing index
    let stats = get_project_stats(&canonical_path).await.ok();

    let mut project = ProjectInfo::new(name.clone(), canonical_path.clone());
    if let Some(s) = stats {
        project.update_stats(s);
        project.mark_indexed();
    }

    registry.add_project(project)?;

    // If this is the first project, make it the default
    if registry.project_count() == 1 {
        registry.set_default(&name)?;
        println!("Added project '{}' at {:?}", name, canonical_path);
        println!("Set as default project (first project added).");
    } else {
        println!("Added project '{}' at {:?}", name, canonical_path);
    }

    registry.save()?;

    Ok(())
}

/// Remove a project from the registry.
pub async fn remove(name: String) -> Result<()> {
    let mut registry = GlobalRegistry::load()?;

    let removed = registry.remove_project(&name);

    match removed {
        Some(project) => {
            registry.save()?;
            println!("Removed project '{}' from registry.", name);
            println!("Path: {:?}", project.path);
            println!("\nNote: The project files and index were not deleted.");
        }
        None => {
            bail!(
                "Project '{}' not found in registry.\n\
                 Use 'coderag projects list' to see all registered projects.",
                name
            );
        }
    }

    Ok(())
}

/// Switch to a different default project.
pub async fn switch(name: String) -> Result<()> {
    let mut registry = GlobalRegistry::load()?;

    // Get the project first to validate it exists
    let project = registry
        .get_project(&name)
        .ok_or_else(|| anyhow::anyhow!(
            "Project '{}' not found in registry.\n\
             Use 'coderag projects list' to see all registered projects.",
            name
        ))?;

    // Check if the path still exists
    if !project.path_exists() {
        bail!(
            "Project '{}' path no longer exists: {:?}\n\
             Consider removing it with 'coderag projects remove {}'",
            name,
            project.path,
            name
        );
    }

    registry.set_default(&name)?;
    registry.save()?;

    let project = registry.get_project(&name).unwrap();
    println!("Switched to project '{}'", name);
    println!("Path: {:?}", project.path);

    Ok(())
}

/// Show the status of the current project.
pub async fn status() -> Result<()> {
    let current_dir = env::current_dir()?;
    let registry = GlobalRegistry::load()?;

    // Try to find a project for the current directory
    let project_from_cwd = registry.find_by_path(&current_dir);

    // Also get the default project
    let default_project = registry.get_default();

    println!("Current directory: {:?}\n", current_dir);

    // Show current directory project status
    if let Some(project) = project_from_cwd {
        print_project_details("Current directory project", project)?;
    } else if Config::is_initialized(&current_dir) {
        println!("Current directory has CodeRAG initialized but is not registered.");
        println!("Run 'coderag projects add <name>' to register it.\n");
    } else {
        println!("Current directory is not a CodeRAG project.");
        println!("Run 'coderag init' to initialize it.\n");
    }

    // Show default project if different
    if let Some(default) = default_project {
        let is_current = project_from_cwd
            .map(|p| p.name == default.name)
            .unwrap_or(false);

        if !is_current {
            println!("---\n");
            print_project_details("Default project", default)?;
        }
    } else {
        println!("No default project set.");
        println!("Use 'coderag projects switch <name>' to set one.");
    }

    // Show global registry info
    println!("\n---");
    println!("Global registry: {:?}", GlobalRegistry::registry_path()?);
    println!("Total projects: {}", registry.project_count());

    Ok(())
}

/// Print detailed information about a project.
fn print_project_details(label: &str, project: &ProjectInfo) -> Result<()> {
    println!("{}: {}", label, project.name);
    println!("  Path: {:?}", project.path);

    if project.path_exists() {
        println!("  Status: OK");
    } else {
        println!("  Status: PATH MISSING");
    }

    println!("  Created: {}", project.created_at.format("%Y-%m-%d %H:%M:%S UTC"));

    if let Some(ref last_indexed) = project.last_indexed {
        println!("  Last indexed: {}", last_indexed.format("%Y-%m-%d %H:%M:%S UTC"));
    } else {
        println!("  Last indexed: Never");
    }

    if let Some(ref stats) = project.stats {
        println!("  Files: {}", stats.file_count);
        println!("  Chunks: {}", stats.chunk_count);
        println!("  Index size: {}", format_bytes(stats.index_size_bytes));
    }

    Ok(())
}

/// Get project statistics from the index.
async fn get_project_stats(project_path: &Path) -> Result<ProjectStats> {
    let config = Config::load(project_path)?;
    let db_path = config.db_path(project_path);

    if !db_path.exists() {
        bail!("Index not found at {:?}", db_path);
    }

    let storage = Storage::new(&db_path).await?;

    let chunk_count = storage.count_chunks().await?;
    let files = storage.list_files(None).await?;
    let file_count = files.len();

    // Calculate index size
    let index_size_bytes = calculate_dir_size(&db_path)?;

    Ok(ProjectStats::new(file_count, chunk_count, index_size_bytes))
}

/// Calculate the total size of a directory.
fn calculate_dir_size(path: &PathBuf) -> Result<u64> {
    let mut total_size = 0u64;

    if path.is_file() {
        return Ok(path.metadata()?.len());
    }

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }

    Ok(total_size)
}

/// Format bytes in a human-readable way.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Truncate a string to a maximum length, adding "..." if truncated.
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

/// Truncate a path to a maximum length, showing the end.
fn truncate_path(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("...{}", &s[s.len() - max_len + 3..])
    } else {
        s[s.len() - max_len..].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("hello", 10), "hello");
        assert_eq!(truncate_string("hello world", 8), "hello...");
        assert_eq!(truncate_string("hi", 2), "hi");
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("/short/path", 20), "/short/path");
        // "/very/long/path/here" is 20 chars, max 15 means we keep 12 chars (15-3 for "...")
        // The last 12 chars are "ng/path/here"
        assert_eq!(truncate_path("/very/long/path/here", 15), "...ng/path/here");
    }
}
