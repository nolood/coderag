use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use coderag::cli::{Cli, Commands, ProjectsCommand};
use coderag::metrics;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "coderag=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    // Register Prometheus metrics
    metrics::register_metrics();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            coderag::commands::init::run().await?;
        }
        Commands::Index => {
            coderag::commands::index::run().await?;
        }
        Commands::Serve { transport, port } => {
            coderag::commands::serve::run(&transport, Some(port)).await?;
        }
        Commands::Search { query, limit } => {
            coderag::commands::search::run(&query, limit).await?;
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
    }

    Ok(())
}
