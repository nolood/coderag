//! Auto-indexing module for zero-ceremony code search.
//!
//! This module provides automatic project detection and indexing, enabling
//! users to run `coderag search "query"` from any project directory without
//! explicit initialization.
//!
//! # Overview
//!
//! The auto-indexing system handles:
//! - **Project Detection**: Finding project roots by looking for markers like
//!   `.git`, `Cargo.toml`, `package.json`, etc.
//! - **Storage Resolution**: Deciding where to store indexes (locally in `.coderag/`
//!   or globally in `~/.local/share/coderag/indexes/`)
//! - **Policy-Based Indexing**: Determining when to index based on configurable policies
//! - **Parallel Indexing**: Efficient multi-threaded file processing
//!
//! # Usage
//!
//! ```ignore
//! use coderag::auto_index::{AutoIndexService, AutoIndexPolicy};
//!
//! // Create service with default policy (OnMissingOrStale)
//! let service = AutoIndexService::new();
//!
//! // Or with a specific policy
//! let service = AutoIndexService::with_policy(AutoIndexPolicy::OnMissing);
//!
//! // Ensure index exists for current directory
//! let result = service.ensure_indexed(&std::env::current_dir()?).await?;
//!
//! println!("Indexed {} files", result.files_indexed);
//! ```
//!
//! # Storage Locations
//!
//! - **Local**: `{project}/.coderag/index.lance` - Used when project has `.coderag/` directory
//! - **Global**: `~/.local/share/coderag/indexes/{project-id}/index.lance` - Used for all other projects

mod service;
mod storage_resolver;

pub use service::{AutoIndexError, AutoIndexPolicy, AutoIndexResult, AutoIndexService};
pub use storage_resolver::{
    compute_project_id, sanitize_name, StorageError, StorageLocation, StorageResolver,
};
