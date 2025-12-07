use anyhow::Result;
use std::env;
use tracing::info;

use crate::Config;

/// Initialize CodeRAG with local configuration.
///
/// Note: `init` is now optional. Projects auto-index globally without it.
/// Running `init` creates a local `.coderag/` directory, which:
/// - Stores the index locally (vs globally in ~/.local/share/coderag/)
/// - Allows custom configuration via `.coderag/config.toml`
///
/// # Arguments
///
/// * `force` - Force reinitialization even if already initialized
pub async fn run(force: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    if Config::is_initialized(&cwd) {
        if force {
            // Remove existing config and recreate
            let coderag_dir = Config::coderag_dir(&cwd);
            std::fs::remove_dir_all(&coderag_dir)?;
            info!("Removed existing configuration at {:?}", coderag_dir);
        } else {
            println!(
                "CodeRAG already initialized locally at {}",
                Config::coderag_dir(&cwd).display()
            );
            println!();
            println!("Note: init is now optional. Projects auto-index globally without it.");
            println!("Use 'coderag search \"query\"' to search immediately in any project.");
            println!();
            println!("Use --force to reinitialize.");
            return Ok(());
        }
    }

    // Create local config
    let config = Config::default();
    config.save(&cwd)?;

    info!("Initialized CodeRAG in {:?}", Config::coderag_dir(&cwd));
    println!(
        "Created {} with default configuration",
        Config::coderag_dir(&cwd).display()
    );
    println!();
    println!("This project will use local storage instead of global.");
    println!();
    println!("Next steps:");
    println!("  1. (Optional) Edit .coderag/config.toml to customize settings");
    println!("  2. Run 'coderag search \"query\"' to search (auto-indexes if needed)");
    println!("  3. Run 'coderag serve' to start the MCP server");
    println!();
    println!("Tip: You can skip init for new projects - CodeRAG auto-indexes globally.");

    Ok(())
}
