use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "coderag")]
#[command(author, version, about = "Semantic code search CLI and MCP server")]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize CodeRAG with local configuration (optional - auto-detection works without this)
    Init {
        /// Force reinitialization even if already initialized
        #[arg(long)]
        force: bool,
    },

    /// Index the codebase (optional - happens automatically on search)
    Index {
        /// Force full re-index, ignoring incremental updates
        #[arg(long)]
        force: bool,
    },

    /// Start the MCP server (auto-indexes if needed)
    Serve {
        /// Use HTTP/SSE transport instead of stdio
        #[arg(long)]
        http: bool,

        /// Port for HTTP transport (default: 3000)
        #[arg(short, long)]
        port: Option<u16>,

        /// Skip auto-indexing on startup
        #[arg(long)]
        no_auto_index: bool,

        /// Enable file watcher for automatic re-indexing
        #[arg(long)]
        watch: bool,

        /// Debounce delay in milliseconds for file watcher (default: 500)
        #[arg(long, default_value = "500")]
        debounce_ms: u64,
    },

    /// Search the codebase (auto-indexes if needed)
    Search {
        /// Search query
        query: String,

        /// Maximum number of results to return
        #[arg(short, long, default_value = "10")]
        limit: Option<usize>,

        /// Skip auto-indexing before search
        #[arg(long)]
        no_auto_index: bool,
    },

    /// Watch for file changes and automatically re-index
    Watch {
        /// Debounce delay in milliseconds
        #[arg(short, long, default_value = "500")]
        debounce_ms: u64,
    },

    /// Show index statistics and metrics
    Stats {
        /// Output in Prometheus format
        #[arg(long)]
        prometheus: bool,
    },

    /// Manage multiple projects
    Projects {
        #[command(subcommand)]
        command: ProjectsCommand,
    },

    /// Start web UI for debugging
    Web {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    /// Show project and index status
    Status,

    /// Migrate local .coderag/ storage to global storage
    Migrate {
        /// Keep local .coderag/ directory after migration (only removes index files)
        #[arg(long)]
        keep_local: bool,

        /// Move files instead of copying (faster, but no rollback)
        #[arg(long, short)]
        move_files: bool,
    },
}

/// Subcommands for project management.
#[derive(Subcommand)]
pub enum ProjectsCommand {
    /// List all registered projects
    List,

    /// Add current directory as a project
    Add {
        /// Name for the project
        name: String,
    },

    /// Remove a project from registry
    Remove {
        /// Name of the project to remove
        name: String,
    },

    /// Set default project
    Switch {
        /// Name of the project to switch to
        name: String,
    },

    /// Show current project status
    Status,
}
