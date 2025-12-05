//! Global project registry stored in ~/.coderag/registry.json

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use super::project::ProjectInfo;

const REGISTRY_FILE: &str = "registry.json";
const APP_QUALIFIER: &str = "com";
const APP_ORGANIZATION: &str = "coderag";
const APP_NAME: &str = "coderag";

/// Global project registry stored in ~/.coderag/registry.json
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalRegistry {
    /// Map of project name to project info
    pub projects: HashMap<String, ProjectInfo>,
    /// Name of the default project (if set)
    pub default_project: Option<String>,
}

impl GlobalRegistry {
    /// Get the global coderag directory path (~/.coderag or platform equivalent).
    pub fn global_dir() -> Result<PathBuf> {
        ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME)
            .map(|dirs| dirs.data_dir().to_path_buf())
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))
    }

    /// Get the path to the registry file.
    pub fn registry_path() -> Result<PathBuf> {
        Ok(Self::global_dir()?.join(REGISTRY_FILE))
    }

    /// Load the global registry from disk, or create a new one if it doesn't exist.
    pub fn load() -> Result<Self> {
        let registry_path = Self::registry_path()?;

        if registry_path.exists() {
            let content = fs::read_to_string(&registry_path)
                .with_context(|| format!("Failed to read registry from {:?}", registry_path))?;

            serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse registry from {:?}", registry_path))
        } else {
            debug!("No existing registry found, creating new one");
            Ok(Self::default())
        }
    }

    /// Save the registry to disk using atomic file operations.
    pub fn save(&self) -> Result<()> {
        let registry_path = Self::registry_path()?;
        let global_dir = Self::global_dir()?;

        // Ensure the global directory exists
        fs::create_dir_all(&global_dir)
            .with_context(|| format!("Failed to create global directory {:?}", global_dir))?;

        // Serialize to JSON
        let content = serde_json::to_string_pretty(self)
            .with_context(|| "Failed to serialize registry")?;

        // Write to a temporary file first, then rename for atomicity
        let temp_path = registry_path.with_extension("json.tmp");

        let mut file = fs::File::create(&temp_path)
            .with_context(|| format!("Failed to create temp file {:?}", temp_path))?;

        file.write_all(content.as_bytes())
            .with_context(|| "Failed to write registry content")?;

        file.sync_all()
            .with_context(|| "Failed to sync registry file")?;

        // Atomic rename
        fs::rename(&temp_path, &registry_path)
            .with_context(|| format!("Failed to rename temp file to {:?}", registry_path))?;

        debug!("Saved registry to {:?}", registry_path);
        Ok(())
    }

    /// Add a new project to the registry.
    ///
    /// Returns an error if a project with the same name already exists.
    pub fn add_project(&mut self, project: ProjectInfo) -> Result<()> {
        let name = project.name.clone();

        if self.projects.contains_key(&name) {
            anyhow::bail!("Project '{}' already exists in the registry", name);
        }

        // Validate that the path exists
        if !project.path_exists() {
            anyhow::bail!("Project path does not exist: {:?}", project.path);
        }

        info!("Adding project '{}' at {:?}", name, project.path);
        self.projects.insert(name, project);

        Ok(())
    }

    /// Remove a project from the registry by name.
    ///
    /// Returns the removed project if it existed.
    pub fn remove_project(&mut self, name: &str) -> Option<ProjectInfo> {
        let removed = self.projects.remove(name);

        // If the removed project was the default, clear the default
        if let Some(ref default_name) = self.default_project {
            if default_name == name {
                self.default_project = None;
            }
        }

        if removed.is_some() {
            info!("Removed project '{}' from registry", name);
        }

        removed
    }

    /// Get a project by name.
    pub fn get_project(&self, name: &str) -> Option<&ProjectInfo> {
        self.projects.get(name)
    }

    /// Get a mutable reference to a project by name.
    pub fn get_project_mut(&mut self, name: &str) -> Option<&mut ProjectInfo> {
        self.projects.get_mut(name)
    }

    /// Find a project by its path.
    pub fn find_by_path(&self, path: &Path) -> Option<&ProjectInfo> {
        // Canonicalize the search path for comparison
        let canonical = path.canonicalize().ok();

        self.projects.values().find(|p| {
            if let Some(ref search_path) = canonical {
                p.path.canonicalize().ok().as_ref() == Some(search_path)
            } else {
                p.path == path
            }
        })
    }

    /// List all registered projects.
    pub fn list_projects(&self) -> Vec<&ProjectInfo> {
        let mut projects: Vec<_> = self.projects.values().collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        projects
    }

    /// Set the default project.
    ///
    /// Returns an error if the project doesn't exist.
    pub fn set_default(&mut self, name: &str) -> Result<()> {
        if !self.projects.contains_key(name) {
            anyhow::bail!("Project '{}' does not exist in the registry", name);
        }

        self.default_project = Some(name.to_string());
        info!("Set default project to '{}'", name);
        Ok(())
    }

    /// Get the default project.
    pub fn get_default(&self) -> Option<&ProjectInfo> {
        self.default_project
            .as_ref()
            .and_then(|name| self.projects.get(name))
    }

    /// Clear the default project.
    pub fn clear_default(&mut self) {
        self.default_project = None;
    }

    /// Check if a project with the given name exists.
    pub fn has_project(&self, name: &str) -> bool {
        self.projects.contains_key(name)
    }

    /// Get the number of registered projects.
    pub fn project_count(&self) -> usize {
        self.projects.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    /// Update a project's information.
    pub fn update_project<F>(&mut self, name: &str, f: F) -> Result<()>
    where
        F: FnOnce(&mut ProjectInfo),
    {
        let project = self
            .projects
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Project '{}' not found", name))?;

        f(project);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_project(name: &str, path: &Path) -> ProjectInfo {
        ProjectInfo::new(name.to_string(), path.to_path_buf())
    }

    #[test]
    fn test_add_and_get_project() {
        let dir = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        let project = create_test_project("test-project", dir.path());
        registry.add_project(project).unwrap();

        let retrieved = registry.get_project("test-project");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-project");
    }

    #[test]
    fn test_add_duplicate_project() {
        let dir = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        let project1 = create_test_project("test-project", dir.path());
        registry.add_project(project1).unwrap();

        let project2 = create_test_project("test-project", dir.path());
        let result = registry.add_project(project2);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_project_nonexistent_path() {
        let mut registry = GlobalRegistry::default();

        let project = ProjectInfo::new(
            "nonexistent".to_string(),
            PathBuf::from("/nonexistent/path/that/does/not/exist"),
        );
        let result = registry.add_project(project);
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_project() {
        let dir = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        let project = create_test_project("test-project", dir.path());
        registry.add_project(project).unwrap();

        let removed = registry.remove_project("test-project");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "test-project");

        assert!(registry.get_project("test-project").is_none());
    }

    #[test]
    fn test_remove_nonexistent_project() {
        let mut registry = GlobalRegistry::default();
        let removed = registry.remove_project("nonexistent");
        assert!(removed.is_none());
    }

    #[test]
    fn test_set_and_get_default() {
        let dir = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        let project = create_test_project("test-project", dir.path());
        registry.add_project(project).unwrap();

        registry.set_default("test-project").unwrap();

        let default = registry.get_default();
        assert!(default.is_some());
        assert_eq!(default.unwrap().name, "test-project");
    }

    #[test]
    fn test_set_default_nonexistent() {
        let mut registry = GlobalRegistry::default();
        let result = registry.set_default("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_default_project() {
        let dir = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        let project = create_test_project("test-project", dir.path());
        registry.add_project(project).unwrap();
        registry.set_default("test-project").unwrap();

        registry.remove_project("test-project");

        assert!(registry.default_project.is_none());
        assert!(registry.get_default().is_none());
    }

    #[test]
    fn test_list_projects() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        registry.add_project(create_test_project("project-b", dir1.path())).unwrap();
        registry.add_project(create_test_project("project-a", dir2.path())).unwrap();

        let projects = registry.list_projects();
        assert_eq!(projects.len(), 2);
        // Should be sorted alphabetically
        assert_eq!(projects[0].name, "project-a");
        assert_eq!(projects[1].name, "project-b");
    }

    #[test]
    fn test_find_by_path() {
        let dir = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        let project = create_test_project("test-project", dir.path());
        registry.add_project(project).unwrap();

        let found = registry.find_by_path(dir.path());
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test-project");
    }

    #[test]
    fn test_update_project() {
        let dir = tempdir().unwrap();
        let mut registry = GlobalRegistry::default();

        let project = create_test_project("test-project", dir.path());
        registry.add_project(project).unwrap();

        registry.update_project("test-project", |p| {
            p.mark_indexed();
        }).unwrap();

        let updated = registry.get_project("test-project").unwrap();
        assert!(updated.last_indexed.is_some());
    }
}
