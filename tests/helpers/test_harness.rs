use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use anyhow::Result;
use coderag::storage::Storage;
use coderag::Config;

pub struct TestHarness {
    pub temp_dir: TempDir,
    pub storage: Arc<Storage>,
    pub config: coderag::Config,
}

impl TestHarness {
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let config = coderag::Config::default();

        // Create storage with temp database (use default dimension for tests)
        let db_path = temp_dir.path().join("test.lance");
        let storage = Arc::new(Storage::new_with_default_dimension(&db_path).await?);

        Ok(Self {
            temp_dir,
            storage,
            config,
        })
    }

    pub fn create_test_file(&self, path: &str, content: &str) -> Result<PathBuf> {
        let file_path = self.temp_dir.path().join(path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, content)?;
        Ok(file_path)
    }

    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }
}