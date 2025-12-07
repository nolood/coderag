//! Multi-project registry module.
//!
//! This module provides support for managing multiple CodeRAG projects
//! through a global registry stored in the user's home directory.
//!
//! # Overview
//!
//! The registry system allows users to:
//! - Register multiple projects with human-readable names
//! - Switch between projects quickly
//! - Track indexing statistics for each project
//! - Set a default project for CLI operations
//!
//! # Storage Location
//!
//! The global registry is stored at:
//! - Linux: `~/.local/share/coderag/registry.json`
//! - macOS: `~/Library/Application Support/com.coderag.coderag/registry.json`
//! - Windows: `C:\Users\<User>\AppData\Roaming\coderag\coderag\data\registry.json`

mod global;
mod project;

pub use global::{GlobalIndexInfo, GlobalRegistry};
pub use project::{ProjectInfo, ProjectStats};
