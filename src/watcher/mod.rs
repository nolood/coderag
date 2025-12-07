//! File system watcher for automatic re-indexing
//!
//! This module provides functionality to watch for file system changes
//! and automatically re-index modified files.

pub mod accumulator;
pub mod batch_detector;
pub mod debouncer;
pub mod git_detector;
pub mod handler;
pub mod parallel_handler;

use anyhow::{Context, Result};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebouncedEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::embeddings::EmbeddingGenerator;
use crate::metrics::{BATCHED_FILES, MASS_CHANGES_DETECTED};
use crate::storage::Storage;

pub use accumulator::{ChangeAccumulator, ChangeType, FileChange};
pub use batch_detector::BatchDetector;
pub use debouncer::ChangeType as DebouncerChangeType;
pub use git_detector::{detect_git_operation_type, is_git_operation, suggest_delay_for_operation, DebouncedEvent as GitDebouncedEvent, GitOp};
pub use handler::{ChangeHandler, ProcessingStats};
pub use parallel_handler::{BatchedEventProcessor, ParallelChangeHandler};

/// Configuration for the file watcher
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce delay in milliseconds
    pub debounce_ms: u64,
    /// File extensions to watch (empty = use indexer config)
    pub extensions: Vec<String>,
    /// Patterns to ignore
    pub ignore_patterns: Vec<String>,
    /// Threshold for mass change detection (number of files)
    pub mass_change_threshold: usize,
    /// Delay for collecting changes during mass operations (milliseconds)
    pub mass_change_delay_ms: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 500,
            extensions: vec![],
            ignore_patterns: vec![],
            mass_change_threshold: 50,
            mass_change_delay_ms: 3000,
        }
    }
}

impl WatcherConfig {
    /// Create a new WatcherConfig from the main Config
    pub fn from_config(config: &Config, debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            extensions: config.indexer.extensions.clone(),
            ignore_patterns: config.indexer.ignore_patterns.clone(),
            mass_change_threshold: 50,
            mass_change_delay_ms: 3000,
        }
    }
}

/// File system watcher for automatic re-indexing
pub struct FileWatcher {
    root: PathBuf,
    config: WatcherConfig,
    storage: Arc<Storage>,
    embedder: Arc<EmbeddingGenerator>,
    app_config: Config,
}

impl FileWatcher {
    /// Create a new file watcher
    pub fn new(
        root: PathBuf,
        config: WatcherConfig,
        storage: Arc<Storage>,
        embedder: Arc<EmbeddingGenerator>,
        app_config: Config,
    ) -> Self {
        Self {
            root,
            config,
            storage,
            embedder,
            app_config,
        }
    }

    /// Start watching for file changes
    ///
    /// This runs until the shutdown signal is received.
    ///
    /// # Arguments
    /// * `shutdown_rx` - Receiver for shutdown signal
    ///
    /// # Returns
    /// Total processing statistics
    pub async fn run(self, mut shutdown_rx: oneshot::Receiver<()>) -> Result<ProcessingStats> {
        let debounce_duration = Duration::from_millis(self.config.debounce_ms);

        // Create channel for receiving debounced events
        let (tx, mut rx) = mpsc::channel::<Vec<DebouncedEvent>>(100);

        // Create the debouncer with notify-debouncer-full
        let tx_clone = tx.clone();
        let mut debouncer = new_debouncer(
            debounce_duration,
            None,
            move |result: std::result::Result<Vec<DebouncedEvent>, Vec<notify::Error>>| {
                match result {
                    Ok(events) => {
                        if !events.is_empty() {
                            if let Err(e) = tx_clone.blocking_send(events) {
                                error!("Failed to send debounced events: {}", e);
                            }
                        }
                    }
                    Err(errors) => {
                        for error in errors {
                            error!("Watch error: {}", error);
                        }
                    }
                }
            },
        )
        .with_context(|| "Failed to create file watcher debouncer")?;

        // Start watching the root directory
        debouncer
            .watch(&self.root, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch directory: {:?}", self.root))?;

        info!("Watching directory: {:?}", self.root);
        info!("Debounce delay: {}ms", self.config.debounce_ms);
        info!("Mass change threshold: {} files", self.config.mass_change_threshold);
        info!("Mass change delay: {}ms", self.config.mass_change_delay_ms);

        // Create change handler
        let mut handler = ChangeHandler::new(
            Arc::clone(&self.storage),
            Arc::clone(&self.embedder),
            self.root.clone(),
            self.app_config.clone(),
        )?;

        // Create batch detector
        let mut batch_detector = BatchDetector::new(
            self.config.mass_change_threshold,
            20.0, // 20 changes per second threshold
            Duration::from_millis(self.config.mass_change_delay_ms),
        );

        // Create change accumulator
        let mut accumulator = ChangeAccumulator::new();

        let mut total_stats = ProcessingStats::default();

        // Event loop
        loop {
            tokio::select! {
                // Check for shutdown signal
                _ = &mut shutdown_rx => {
                    info!("Shutdown signal received, stopping watcher");

                    // Process any accumulated changes before shutting down
                    if !accumulator.is_empty() {
                        let batched_changes = accumulator.flush();
                        info!("Flushing {} batched changes before shutdown", batched_changes.len());
                        if let Ok(stats) = handler.process_changes(batched_changes).await {
                            total_stats.merge(&stats);
                        }
                    }

                    break;
                }

                // Process debounced events
                Some(events) = rx.recv() => {
                    // Convert to Git debounced events for detection
                    let git_events: Vec<git_detector::DebouncedEvent> = events
                        .iter()
                        .map(|e| git_detector::DebouncedEvent::from(e.clone()))
                        .collect();

                    // Check for git operations
                    let is_git = is_git_operation(&git_events);
                    let git_op = if is_git {
                        detect_git_operation_type(&git_events)
                    } else {
                        None
                    };

                    if let Some(op) = &git_op {
                        info!("Git operation detected: {:?}, using extended batching", op);
                    }

                    let changes = self.convert_events(events);

                    if !changes.is_empty() {
                        // Check for mass change
                        if batch_detector.detect_mass_change(changes.len()) || git_op.is_some() {
                            let delay = if let Some(op) = &git_op {
                                Duration::from_millis(suggest_delay_for_operation(op))
                            } else {
                                batch_detector.collection_delay()
                            };

                            info!(
                                "Mass change or git operation detected ({} files), batching with {}ms delay",
                                changes.len(),
                                delay.as_millis()
                            );

                            // Track metrics
                            MASS_CHANGES_DETECTED.inc();

                            // Accumulate changes
                            for change in changes {
                                accumulator.add_change(change);
                            }
                        } else {
                            // Process normally if not mass change and no pending accumulation
                            if accumulator.is_empty() {
                                info!("Processing {} file changes", changes.len());

                                match handler.process_changes(changes).await {
                                    Ok(stats) => {
                                        total_stats.merge(&stats);
                                        Self::print_stats(&stats);
                                    }
                                    Err(e) => {
                                        error!("Failed to process changes: {}", e);
                                        total_stats.errors += 1;
                                    }
                                }
                            } else {
                                // Add to accumulator if already accumulating
                                for change in changes {
                                    accumulator.add_change(change);
                                }
                            }
                        }
                    }
                }

                // Check accumulator periodically
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if accumulator.is_accumulating() {
                        let delay = batch_detector.collection_delay();
                        if accumulator.should_flush(delay) {
                            let batched_changes = accumulator.flush();
                            if !batched_changes.is_empty() {
                                info!("Flushing {} batched changes", batched_changes.len());

                                // Track batch size metric
                                BATCHED_FILES.observe(batched_changes.len() as f64);

                                match handler.process_changes(batched_changes).await {
                                    Ok(stats) => {
                                        total_stats.merge(&stats);
                                        Self::print_stats(&stats);
                                    }
                                    Err(e) => {
                                        error!("Failed to process batched changes: {}", e);
                                        total_stats.errors += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(total_stats)
    }

    /// Convert notify debounced events to our FileChange type
    fn convert_events(&self, events: Vec<DebouncedEvent>) -> Vec<FileChange> {
        let mut changes = Vec::new();
        let now = std::time::Instant::now();

        for event in &events {
            for path in &event.paths {
                // Skip directories
                if path.is_dir() {
                    continue;
                }

                // Check if file matches our extensions
                if !self.should_watch(path) {
                    debug!("Skipping file (not in extensions): {:?}", path);
                    continue;
                }

                // Check if file matches ignore patterns
                if self.is_ignored(path) {
                    debug!("Skipping file (ignored): {:?}", path);
                    continue;
                }

                let change_type = match event.kind {
                    notify::EventKind::Create(_) => ChangeType::Created,
                    notify::EventKind::Modify(_) => ChangeType::Modified,
                    notify::EventKind::Remove(_) => ChangeType::Deleted,
                    _ => continue,
                };

                debug!("File change detected: {:?} -> {:?}", change_type, path);
                changes.push(FileChange {
                    path: path.clone(),
                    change_type,
                    timestamp: now,
                });
            }
        }

        // Deduplicate changes (keep last change type for each path)
        let mut seen = std::collections::HashMap::new();
        for change in changes {
            seen.insert(change.path.clone(), change);
        }

        seen.into_values().collect()
    }

    /// Check if a file should be watched based on extensions
    fn should_watch(&self, path: &std::path::Path) -> bool {
        if self.config.extensions.is_empty() {
            return true;
        }

        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| self.config.extensions.iter().any(|e| e == ext))
            .unwrap_or(false)
    }

    /// Check if a path matches any ignore pattern
    fn is_ignored(&self, path: &std::path::Path) -> bool {
        let path_str = path.to_string_lossy();

        for pattern in &self.config.ignore_patterns {
            if path_str.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Print processing statistics
    fn print_stats(stats: &ProcessingStats) {
        if stats.files_added > 0 {
            println!("  + {} files added", stats.files_added);
        }
        if stats.files_modified > 0 {
            println!("  ~ {} files modified", stats.files_modified);
        }
        if stats.files_deleted > 0 {
            println!("  - {} files deleted", stats.files_deleted);
        }
        if stats.chunks_created > 0 {
            println!("  {} chunks created", stats.chunks_created);
        }
        if stats.chunks_removed > 0 {
            println!("  {} chunks removed", stats.chunks_removed);
        }
        if stats.errors > 0 {
            warn!("  {} errors occurred", stats.errors);
        }
    }
}

/// Handle to control a running watcher
pub struct WatcherHandle {
    shutdown_tx: Option<oneshot::Sender<()>>,
    result_rx: Option<oneshot::Receiver<ProcessingStats>>,
}

impl WatcherHandle {
    /// Create a new watcher handle
    pub fn new(
        shutdown_tx: oneshot::Sender<()>,
        result_rx: oneshot::Receiver<ProcessingStats>,
    ) -> Self {
        Self {
            shutdown_tx: Some(shutdown_tx),
            result_rx: Some(result_rx),
        }
    }

    /// Request graceful shutdown
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Wait for the watcher to finish and get the final stats
    pub async fn wait(mut self) -> Result<ProcessingStats> {
        if let Some(rx) = self.result_rx.take() {
            rx.await.with_context(|| "Watcher task panicked")
        } else {
            Ok(ProcessingStats::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.debounce_ms, 500);
        assert!(config.extensions.is_empty());
        assert!(config.ignore_patterns.is_empty());
    }
}
