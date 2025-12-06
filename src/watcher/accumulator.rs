use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::debug;

/// Represents a file change event
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub timestamp: Instant,
}

/// Types of file changes
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
    Renamed { from: PathBuf },
}

impl fmt::Display for ChangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChangeType::Created => write!(f, "Created"),
            ChangeType::Modified => write!(f, "Modified"),
            ChangeType::Deleted => write!(f, "Deleted"),
            ChangeType::Renamed { .. } => write!(f, "Renamed"),
        }
    }
}

/// Accumulates file changes and deduplicates by path
pub struct ChangeAccumulator {
    /// Map of path to most recent change
    changes: HashMap<PathBuf, FileChange>,
    /// When accumulation started
    accumulation_start: Option<Instant>,
    /// Total changes received (including duplicates)
    total_changes_received: usize,
}

impl ChangeAccumulator {
    pub fn new() -> Self {
        Self {
            changes: HashMap::new(),
            accumulation_start: None,
            total_changes_received: 0,
        }
    }

    /// Adds a change to the accumulator, deduplicating by path
    pub fn add_change(&mut self, change: FileChange) {
        self.total_changes_received += 1;

        // Start accumulation timer if not already started
        if self.accumulation_start.is_none() {
            self.accumulation_start = Some(Instant::now());
            debug!("Started accumulating changes");
        }

        // Handle change based on type
        match &change.change_type {
            ChangeType::Deleted => {
                // Always keep delete operations
                self.changes.insert(change.path.clone(), change);
            }
            ChangeType::Created | ChangeType::Modified => {
                // For create/modify, keep the most recent
                self.changes.insert(change.path.clone(), change);
            }
            ChangeType::Renamed { from } => {
                // Remove the old path if it exists
                self.changes.remove(from);
                // Add the new path
                self.changes.insert(change.path.clone(), change);
            }
        }
    }

    /// Checks if accumulated changes should be flushed based on delay
    pub fn should_flush(&self, delay: Duration) -> bool {
        self.accumulation_start
            .map(|start| start.elapsed() >= delay)
            .unwrap_or(false)
    }

    /// Flushes all accumulated changes and resets the accumulator
    pub fn flush(&mut self) -> Vec<FileChange> {
        debug!(
            "Flushing {} unique changes (received {} total)",
            self.changes.len(),
            self.total_changes_received
        );

        self.accumulation_start = None;
        self.total_changes_received = 0;

        // Extract changes, sorted by path for consistency
        let mut changes: Vec<_> = self.changes.drain().map(|(_, v)| v).collect();
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        changes
    }

    /// Returns the number of unique changes accumulated
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Checks if accumulator is empty
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns true if accumulation is active
    pub fn is_accumulating(&self) -> bool {
        self.accumulation_start.is_some()
    }

    /// Returns the elapsed time since accumulation started
    pub fn elapsed(&self) -> Option<Duration> {
        self.accumulation_start.map(|start| start.elapsed())
    }

    /// Clears all accumulated changes without returning them
    pub fn clear(&mut self) {
        self.changes.clear();
        self.accumulation_start = None;
        self.total_changes_received = 0;
    }
}

impl Default for ChangeAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_deduplication() {
        let mut acc = ChangeAccumulator::new();
        let path = PathBuf::from("/test/file.rs");

        // Add multiple changes for the same file
        acc.add_change(FileChange {
            path: path.clone(),
            change_type: ChangeType::Created,
            timestamp: Instant::now(),
        });

        thread::sleep(Duration::from_millis(10));

        acc.add_change(FileChange {
            path: path.clone(),
            change_type: ChangeType::Modified,
            timestamp: Instant::now(),
        });

        // Should only have one entry
        assert_eq!(acc.len(), 1);
        assert_eq!(acc.total_changes_received, 2);

        let flushed = acc.flush();
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn test_rename_handling() {
        let mut acc = ChangeAccumulator::new();
        let old_path = PathBuf::from("/test/old.rs");
        let new_path = PathBuf::from("/test/new.rs");

        // Add original file
        acc.add_change(FileChange {
            path: old_path.clone(),
            change_type: ChangeType::Created,
            timestamp: Instant::now(),
        });

        // Rename it
        acc.add_change(FileChange {
            path: new_path.clone(),
            change_type: ChangeType::Renamed { from: old_path.clone() },
            timestamp: Instant::now(),
        });

        let flushed = acc.flush();
        assert_eq!(flushed.len(), 1);
        assert_eq!(flushed[0].path, new_path);
    }

    #[test]
    fn test_should_flush() {
        let mut acc = ChangeAccumulator::new();
        let delay = Duration::from_millis(50);

        // Should not flush when empty
        assert!(!acc.should_flush(delay));

        // Add a change
        acc.add_change(FileChange {
            path: PathBuf::from("/test/file.rs"),
            change_type: ChangeType::Created,
            timestamp: Instant::now(),
        });

        // Should not flush immediately
        assert!(!acc.should_flush(delay));

        // Wait for delay
        thread::sleep(delay);

        // Should flush now
        assert!(acc.should_flush(delay));
    }

    #[test]
    fn test_accumulation_tracking() {
        let mut acc = ChangeAccumulator::new();

        assert!(!acc.is_accumulating());
        assert!(acc.elapsed().is_none());

        acc.add_change(FileChange {
            path: PathBuf::from("/test/file.rs"),
            change_type: ChangeType::Created,
            timestamp: Instant::now(),
        });

        assert!(acc.is_accumulating());
        assert!(acc.elapsed().is_some());

        acc.flush();

        assert!(!acc.is_accumulating());
        assert!(acc.elapsed().is_none());
    }
}