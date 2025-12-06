use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::indexer::ChunkerStrategy;

const CONFIG_DIR: &str = ".coderag";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub indexer: IndexerConfig,

    #[serde(default)]
    pub embeddings: EmbeddingsConfig,

    #[serde(default)]
    pub storage: StorageConfig,

    #[serde(default)]
    pub server: ServerConfig,

    #[serde(default)]
    pub search: SearchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    /// File extensions to index
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,

    /// Patterns to ignore (in addition to .gitignore)
    #[serde(default = "default_ignore_patterns")]
    pub ignore_patterns: Vec<String>,

    /// Approximate chunk size in tokens
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,

    /// Chunking strategy: "ast" (default) or "line"
    #[serde(default)]
    pub chunker_strategy: ChunkerStrategy,

    /// Minimum tokens for a semantic unit (smaller units get merged)
    #[serde(default = "default_min_chunk_tokens")]
    pub min_chunk_tokens: usize,

    /// Maximum tokens for a semantic unit (larger units use line chunking)
    #[serde(default = "default_max_chunk_tokens")]
    pub max_chunk_tokens: usize,

    /// Number of parallel threads for indexing (None = auto-detect)
    #[serde(default)]
    pub parallel_threads: Option<usize>,

    /// Number of files to process in parallel batches
    #[serde(default = "default_file_batch_size")]
    pub file_batch_size: usize,

    /// Maximum number of concurrent file operations
    #[serde(default = "default_max_concurrent_files")]
    pub max_concurrent_files: usize,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            extensions: default_extensions(),
            ignore_patterns: default_ignore_patterns(),
            chunk_size: default_chunk_size(),
            chunker_strategy: ChunkerStrategy::default(),
            min_chunk_tokens: default_min_chunk_tokens(),
            max_chunk_tokens: default_max_chunk_tokens(),
            parallel_threads: None,
            file_batch_size: default_file_batch_size(),
            max_concurrent_files: default_max_concurrent_files(),
        }
    }
}

fn default_min_chunk_tokens() -> usize {
    50
}

fn default_max_chunk_tokens() -> usize {
    1500
}

fn default_extensions() -> Vec<String> {
    vec![
        "rs".to_string(),
        "py".to_string(),
        "ts".to_string(),
        "tsx".to_string(),
        "js".to_string(),
        "jsx".to_string(),
        "go".to_string(),
        "java".to_string(),
        "c".to_string(),
        "cc".to_string(),
        "cpp".to_string(),
        "cxx".to_string(),
        "h".to_string(),
        "hpp".to_string(),
        "hxx".to_string(),
    ]
}

fn default_ignore_patterns() -> Vec<String> {
    vec![
        "node_modules".to_string(),
        "target".to_string(),
        ".git".to_string(),
        "dist".to_string(),
        "build".to_string(),
        "__pycache__".to_string(),
        ".venv".to_string(),
        "vendor".to_string(),
    ]
}

fn default_chunk_size() -> usize {
    512
}

fn default_file_batch_size() -> usize {
    100
}

fn default_max_concurrent_files() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    /// Embedding model name
    #[serde(default = "default_model")]
    pub model: String,

    /// Batch size for embedding generation
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            batch_size: default_batch_size(),
        }
    }
}

fn default_model() -> String {
    "nomic-embed-text-v1.5".to_string()
}

fn default_batch_size() -> usize {
    32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to the LanceDB database (relative to .coderag/)
    #[serde(default = "default_db_path")]
    pub db_path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
        }
    }
}

fn default_db_path() -> String {
    "index.lance".to_string()
}

/// Transport type for MCP server
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    /// Standard input/output transport (default)
    Stdio,
    /// HTTP/SSE transport for remote access
    Http,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Stdio
    }
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdio => write!(f, "stdio"),
            Self::Http => write!(f, "http"),
        }
    }
}

/// MCP server configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server transport type
    #[serde(default)]
    pub transport: TransportType,

    /// HTTP transport configuration
    #[serde(default)]
    pub http: HttpServerConfig,
}

/// HTTP server configuration for MCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpServerConfig {
    /// Host to bind to
    #[serde(default = "default_http_host")]
    pub host: String,

    /// Port to bind to
    #[serde(default = "default_http_port")]
    pub port: u16,

    /// SSE endpoint path
    #[serde(default = "default_sse_path")]
    pub sse_path: String,

    /// Message endpoint path
    #[serde(default = "default_post_path")]
    pub post_path: String,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            host: default_http_host(),
            port: default_http_port(),
            sse_path: default_sse_path(),
            post_path: default_post_path(),
        }
    }
}

fn default_http_host() -> String {
    "127.0.0.1".to_string()
}

fn default_http_port() -> u16 {
    3000
}

fn default_sse_path() -> String {
    "/sse".to_string()
}

fn default_post_path() -> String {
    "/message".to_string()
}

/// Search mode configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Vector-only semantic search
    Vector,
    /// BM25-only keyword search
    Bm25,
    /// Hybrid search combining vector and BM25
    Hybrid,
}

impl Default for SearchMode {
    fn default() -> Self {
        Self::Hybrid
    }
}

impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchMode::Vector => write!(f, "vector"),
            SearchMode::Bm25 => write!(f, "bm25"),
            SearchMode::Hybrid => write!(f, "hybrid"),
        }
    }
}

/// Search configuration for hybrid search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Search mode: vector, bm25, or hybrid
    #[serde(default)]
    pub mode: SearchMode,

    /// Weight for vector search results (0.0 - 1.0)
    #[serde(default = "default_vector_weight")]
    pub vector_weight: f32,

    /// Weight for BM25 search results (0.0 - 1.0)
    #[serde(default = "default_bm25_weight")]
    pub bm25_weight: f32,

    /// RRF k constant (higher = smoother rank influence)
    #[serde(default = "default_rrf_k")]
    pub rrf_k: f32,

    /// Default number of results to return
    #[serde(default = "default_search_limit")]
    pub default_limit: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            mode: SearchMode::default(),
            vector_weight: default_vector_weight(),
            bm25_weight: default_bm25_weight(),
            rrf_k: default_rrf_k(),
            default_limit: default_search_limit(),
        }
    }
}

fn default_vector_weight() -> f32 {
    0.7
}

fn default_bm25_weight() -> f32 {
    0.3
}

fn default_rrf_k() -> f32 {
    60.0
}

fn default_search_limit() -> usize {
    10
}

impl Config {
    /// Load configuration from the .coderag directory
    pub fn load(root: &Path) -> Result<Self> {
        let config_path = root.join(CONFIG_DIR).join(CONFIG_FILE);

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config from {:?}", config_path))?;

            toml::from_str(&content)
                .with_context(|| format!("Failed to parse config from {:?}", config_path))
        } else {
            Ok(Config::default())
        }
    }

    /// Save configuration to the .coderag directory
    pub fn save(&self, root: &Path) -> Result<()> {
        let config_dir = root.join(CONFIG_DIR);
        let config_path = config_dir.join(CONFIG_FILE);

        std::fs::create_dir_all(&config_dir)
            .with_context(|| format!("Failed to create config directory {:?}", config_dir))?;

        let content =
            toml::to_string_pretty(self).with_context(|| "Failed to serialize config")?;

        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config to {:?}", config_path))?;

        Ok(())
    }

    /// Get the path to the .coderag directory
    pub fn coderag_dir(root: &Path) -> PathBuf {
        root.join(CONFIG_DIR)
    }

    /// Get the path to the LanceDB database
    pub fn db_path(&self, root: &Path) -> PathBuf {
        Self::coderag_dir(root).join(&self.storage.db_path)
    }

    /// Check if CodeRAG is initialized in the given directory
    pub fn is_initialized(root: &Path) -> bool {
        Self::coderag_dir(root).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.indexer.extensions.contains(&"rs".to_string()));
        assert!(config.indexer.extensions.contains(&"py".to_string()));
        assert_eq!(config.indexer.chunk_size, 512);
        assert_eq!(config.embeddings.model, "nomic-embed-text-v1.5");
        // Search config defaults
        assert_eq!(config.search.mode, SearchMode::Hybrid);
        assert!((config.search.vector_weight - 0.7).abs() < 0.001);
        assert!((config.search.bm25_weight - 0.3).abs() < 0.001);
        assert!((config.search.rrf_k - 60.0).abs() < 0.001);
        assert_eq!(config.search.default_limit, 10);
    }

    #[test]
    fn test_save_and_load_config() {
        let dir = tempdir().unwrap();
        let config = Config::default();

        config.save(dir.path()).unwrap();
        let loaded = Config::load(dir.path()).unwrap();

        assert_eq!(config.indexer.extensions, loaded.indexer.extensions);
        assert_eq!(config.embeddings.model, loaded.embeddings.model);
    }

    #[test]
    fn test_load_missing_config_returns_default() {
        let dir = tempdir().unwrap();
        let config = Config::load(dir.path()).unwrap();

        assert_eq!(config.indexer.chunk_size, 512);
    }
}
