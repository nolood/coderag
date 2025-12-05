pub mod cli;
pub mod commands;
pub mod config;
pub mod embeddings;
pub mod indexer;
pub mod mcp;
pub mod metrics;
pub mod registry;
pub mod search;
pub mod storage;
pub mod watcher;
pub mod web;

pub use config::Config;
pub use registry::{GlobalRegistry, ProjectInfo, ProjectStats};
