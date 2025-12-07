use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

use coderag::cli::{Cli, Commands, ProjectsCommand};
use coderag::config::Config;
use coderag::logging::init_logging;
use coderag::metrics;

#[tokio::main]
async fn main() -> Result<()> {
    // Determine project root (current directory)
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Load configuration (if available, otherwise use defaults)
    let config = Config::load(&project_root).unwrap_or_default();

    // Initialize logging with configuration
    // The guard MUST be held until program exit to ensure logs are flushed
    let _logging_guard = init_logging(&config.logging, &project_root)?;

    tracing::info!("CodeRAG starting up");
    tracing::debug!("Loaded configuration from: {}", project_root.display());

    // Register Prometheus metrics
    metrics::register_metrics();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => {
            coderag::commands::init::run(force).await?;
        }
        Commands::Index { force } => {
            coderag::commands::index::run(force).await?;
        }
        Commands::Serve {
            http,
            port,
            no_auto_index,
            watch,
            debounce_ms,
        } => {
            coderag::commands::serve::run(http, port, no_auto_index, watch, debounce_ms).await?;
        }
        Commands::Search {
            query,
            limit,
            no_auto_index,
        } => {
            coderag::commands::search::run(&query, limit, no_auto_index).await?;
        }
        Commands::Watch { debounce_ms } => {
            coderag::commands::watch::run(debounce_ms).await?;
        }
        Commands::Stats { prometheus } => {
            coderag::commands::stats::run(prometheus).await?;
        }
        Commands::Projects { command } => match command {
            ProjectsCommand::List => {
                coderag::commands::projects::list().await?;
            }
            ProjectsCommand::Add { name } => {
                coderag::commands::projects::add(name).await?;
            }
            ProjectsCommand::Remove { name } => {
                coderag::commands::projects::remove(name).await?;
            }
            ProjectsCommand::Switch { name } => {
                coderag::commands::projects::switch(name).await?;
            }
            ProjectsCommand::Status => {
                coderag::commands::projects::status().await?;
            }
        },
        Commands::Web { port } => {
            coderag::commands::web::run(port).await?;
        }
        Commands::Status => {
            coderag::commands::status::run().await?;
        }
        Commands::Migrate {
            keep_local,
            move_files,
        } => {
            coderag::commands::migrate::run(keep_local, move_files).await?;
        }
    }

    Ok(())
}
