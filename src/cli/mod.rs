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
    /// Initialize CodeRAG in the current directory
    Init,

    /// Index or re-index the codebase
    Index,

    /// Start the MCP server
    Serve {
        /// Transport type: stdio (default) or http
        #[arg(short, long, default_value = "stdio")]
        transport: String,

        /// Port for HTTP transport (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },

    /// Search the codebase
    Search {
        /// Search query
        query: String,

        /// Maximum number of results to return
        #[arg(short, long, default_value = "10")]
        limit: Option<usize>,
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
