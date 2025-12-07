pub mod auto_index;
pub mod cli;
pub mod commands;
pub mod config;
pub mod embeddings;
pub mod indexer;
pub mod indexing;
pub mod logging;
pub mod mcp;
pub mod metrics;
pub mod project_detection;
pub mod registry;
pub mod search;
pub mod storage;
pub mod symbol;
pub mod watcher;
pub mod web;

pub use auto_index::{
    compute_project_id, sanitize_name, AutoIndexError, AutoIndexPolicy, AutoIndexResult,
    AutoIndexService, StorageError, StorageLocation, StorageResolver,
};
pub use config::Config;
pub use project_detection::{DetectedProject, DetectionError, ProjectDetector, ProjectType};
pub use registry::{GlobalIndexInfo, GlobalRegistry, ProjectInfo, ProjectStats};
