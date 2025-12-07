//! Watch command implementation
//!
//! Watches for file changes and automatically re-indexes modified files.

use anyhow::{bail, Result};
use std::env;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::oneshot;
use tracing::info;

use crate::embeddings::EmbeddingGenerator;
use crate::storage::Storage;
use crate::watcher::{FileWatcher, WatcherConfig};
use crate::Config;

/// Run the watch command
///
/// # Arguments
/// * `debounce_ms` - Debounce delay in milliseconds
pub async fn run(debounce_ms: u64) -> Result<()> {
    let root = env::current_dir()?;

    if !Config::is_initialized(&root) {
        bail!("CodeRAG is not initialized. Run 'coderag init' first.");
    }

    let config = Config::load(&root)?;

    println!("Starting watch mode...");
    println!("Watching directory: {:?}", root);
    println!("Debounce delay: {}ms", debounce_ms);
    println!("Extensions: {:?}", config.indexer.extensions);
    println!();
    println!("Press Ctrl+C to stop.");
    println!();

    // Initialize components
    let embedder = Arc::new(EmbeddingGenerator::new_async(&config.embeddings).await?);
    let vector_dimension = embedder.embedding_dimension();
    let storage = Arc::new(Storage::new(&config.db_path(&root), vector_dimension).await?);

    // Create watcher config
    let watcher_config = WatcherConfig::from_config(&config, debounce_ms);

    // Create the watcher
    let watcher = FileWatcher::new(
        root,
        watcher_config,
        storage,
        embedder,
        config,
    );

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // Spawn the watcher task
    let watcher_handle = tokio::spawn(async move {
        watcher.run(shutdown_rx).await
    });

    // Wait for Ctrl+C
    signal::ctrl_c().await?;

    println!();
    println!("Shutting down...");

    // Send shutdown signal
    let _ = shutdown_tx.send(());

    // Wait for watcher to finish
    let stats = watcher_handle.await??;

    // Print final statistics
    println!();
    println!("Watch session complete!");
    println!("----------------------------------------");
    println!("  Files added:    {}", stats.files_added);
    println!("  Files modified: {}", stats.files_modified);
    println!("  Files deleted:  {}", stats.files_deleted);
    println!("  Chunks created: {}", stats.chunks_created);
    println!("  Chunks removed: {}", stats.chunks_removed);
    if stats.errors > 0 {
        println!("  Errors:         {}", stats.errors);
    }
    println!("----------------------------------------");

    info!("Watch session ended");

    Ok(())
}
