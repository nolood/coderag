use ignore::WalkBuilder;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;

use crate::config::IndexerConfig;

/// Walks the filesystem respecting .gitignore and custom ignore patterns
pub struct Walker {
    root: PathBuf,
    extensions: HashSet<String>,
    ignore_patterns: Vec<String>,
}

impl Walker {
    /// Create a new Walker with the given root directory and configuration
    pub fn new(root: PathBuf, config: &IndexerConfig) -> Self {
        Self {
            root,
            extensions: config.extensions.iter().cloned().collect(),
            ignore_patterns: config.ignore_patterns.clone(),
        }
    }

    /// Walk the directory tree and return an iterator of file paths
    ///
    /// This respects:
    /// - .gitignore files
    /// - Custom ignore patterns from config
    /// - File extension filtering
    pub fn walk(&self) -> impl Iterator<Item = PathBuf> {
        let mut builder = WalkBuilder::new(&self.root);

        // Enable .gitignore support (enabled by default, but explicit)
        builder.git_ignore(true);
        builder.git_global(true);
        builder.git_exclude(true);

        // Add hidden file filtering (skip .git, etc.)
        builder.hidden(true);

        // Add custom ignore patterns using overrides
        let mut override_builder = ignore::overrides::OverrideBuilder::new(&self.root);
        for pattern in &self.ignore_patterns {
            // Negate pattern to ignore (! prefix means include, so we use !pattern to exclude)
            let _ = override_builder.add(&format!("!{}", pattern));
            let _ = override_builder.add(&format!("!{}/**", pattern));
        }

        if let Ok(overrides) = override_builder.build() {
            builder.overrides(overrides);
        }

        let extensions = self.extensions.clone();
        let ignore_patterns = self.ignore_patterns.clone();

        builder
            .build()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .filter(move |entry| {
                // Check if any ignore pattern matches the path
                let path_str = entry.path().to_string_lossy();
                for pattern in &ignore_patterns {
                    if path_str.contains(pattern) {
                        return false;
                    }
                }
                true
            })
            .filter(move |entry| {
                entry
                    .path()
                    .extension()
                    .and_then(OsStr::to_str)
                    .map(|ext| extensions.contains(ext))
                    .unwrap_or(false)
            })
            .map(|entry| entry.into_path())
    }

    /// Get the number of files that will be walked (for progress bars)
    pub fn count_files(&self) -> usize {
        self.walk().count()
    }

    /// Collect all walkable files into a Vec
    pub fn collect_files(&self) -> Vec<PathBuf> {
        self.walk().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn test_config() -> IndexerConfig {
        IndexerConfig {
            extensions: vec!["rs".to_string(), "py".to_string()],
            ignore_patterns: vec!["target".to_string()],
            chunk_size: 512,
            ..Default::default()
        }
    }

    #[test]
    fn test_walker_finds_rust_files() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();

        fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
        fs::write(src_dir.join("lib.rs"), "pub fn foo() {}").unwrap();
        fs::write(src_dir.join("readme.md"), "# Readme").unwrap();

        let walker = Walker::new(dir.path().to_path_buf(), &test_config());
        let files: Vec<_> = walker.collect_files();

        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.extension().unwrap() == "rs"));
    }

    #[test]
    fn test_walker_respects_extensions() {
        let dir = tempdir().unwrap();

        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("script.py"), "print('hello')").unwrap();
        fs::write(dir.path().join("index.js"), "console.log('hi')").unwrap();

        let walker = Walker::new(dir.path().to_path_buf(), &test_config());
        let files: Vec<_> = walker.collect_files();

        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_walker_ignores_directories() {
        let dir = tempdir().unwrap();
        let target_dir = dir.path().join("target");
        fs::create_dir_all(&target_dir).unwrap();

        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(target_dir.join("build.rs"), "fn build() {}").unwrap();

        let mut config = test_config();
        config.ignore_patterns = vec!["target".to_string()];

        let walker = Walker::new(dir.path().to_path_buf(), &config);
        let files: Vec<_> = walker.collect_files();

        // Should only find main.rs, not target/build.rs
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("main.rs"));
    }
}
