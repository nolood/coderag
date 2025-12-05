use anyhow::{bail, Result};
use std::env;
use tracing::info;

use crate::Config;

pub async fn run() -> Result<()> {
    let root = env::current_dir()?;

    if Config::is_initialized(&root) {
        bail!(
            "CodeRAG is already initialized in {:?}",
            Config::coderag_dir(&root)
        );
    }

    let config = Config::default();
    config.save(&root)?;

    info!(
        "Initialized CodeRAG in {:?}",
        Config::coderag_dir(&root)
    );
    println!(
        "✓ Created {} with default configuration",
        Config::coderag_dir(&root).display()
    );
    println!("\nNext steps:");
    println!("  1. Edit .coderag/config.toml to customize settings");
    println!("  2. Run 'coderag index' to index your codebase");
    println!("  3. Run 'coderag serve' to start the MCP server");

    Ok(())
}
