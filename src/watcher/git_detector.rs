use notify::Event;
use std::path::PathBuf;
use tracing::debug;

/// Types of Git operations that can be detected
#[derive(Debug, Clone, PartialEq)]
pub enum GitOp {
    Checkout,
    Merge,
    Rebase,
    Pull,
    Reset,
    Stash,
}

/// Represents a debounced file system event
#[derive(Debug, Clone)]
pub struct DebouncedEvent {
    pub paths: Vec<PathBuf>,
    pub kind: notify::EventKind,
}

impl From<Event> for DebouncedEvent {
    fn from(event: Event) -> Self {
        Self {
            paths: event.paths,
            kind: event.kind,
        }
    }
}

impl From<notify_debouncer_full::DebouncedEvent> for DebouncedEvent {
    fn from(event: notify_debouncer_full::DebouncedEvent) -> Self {
        Self {
            paths: event.paths.clone(),
            kind: event.kind,
        }
    }
}

/// Detects if file system events are related to Git operations
pub fn is_git_operation(events: &[DebouncedEvent]) -> bool {
    events.iter().any(|event| {
        event.paths.iter().any(|path| {
            // Check if any path component is .git
            path.components()
                .any(|c| c.as_os_str() == ".git")
        })
    })
}

/// Detects the type of Git operation based on file system events
pub fn detect_git_operation_type(events: &[DebouncedEvent]) -> Option<GitOp> {
    let git_paths: Vec<_> = events
        .iter()
        .flat_map(|e| &e.paths)
        .filter(|p| p.components().any(|c| c.as_os_str() == ".git"))
        .collect();

    if git_paths.is_empty() {
        return None;
    }

    // Check for specific Git operation indicators
    for path in &git_paths {
        let path_str = path.to_string_lossy();

        // Check for HEAD changes (checkout/branch switch)
        if path_str.contains(".git/HEAD") || path_str.contains(".git/refs/heads") {
            debug!("Git checkout detected from HEAD change");
            return Some(GitOp::Checkout);
        }

        // Check for merge indicators
        if path_str.contains(".git/MERGE_HEAD") || path_str.contains(".git/MERGE_MSG") {
            debug!("Git merge detected");
            return Some(GitOp::Merge);
        }

        // Check for rebase indicators
        if path_str.contains(".git/rebase-merge") || path_str.contains(".git/rebase-apply") {
            debug!("Git rebase detected");
            return Some(GitOp::Rebase);
        }

        // Check for fetch/pull indicators
        if path_str.contains(".git/FETCH_HEAD") {
            debug!("Git pull/fetch detected");
            return Some(GitOp::Pull);
        }

        // Check for reset indicators
        if path_str.contains(".git/ORIG_HEAD") {
            debug!("Git reset detected");
            return Some(GitOp::Reset);
        }

        // Check for stash indicators
        if path_str.contains(".git/refs/stash") {
            debug!("Git stash operation detected");
            return Some(GitOp::Stash);
        }
    }

    // If .git/index is modified but no specific operation detected, it's likely a generic operation
    if git_paths.iter().any(|p| p.to_string_lossy().contains(".git/index")) {
        debug!("Generic git operation detected from index change");
        Some(GitOp::Checkout)
    } else {
        None
    }
}

/// Suggests an appropriate delay for the detected Git operation
pub fn suggest_delay_for_operation(op: &GitOp) -> u64 {
    match op {
        GitOp::Checkout => 5000,  // 5 seconds for branch switches
        GitOp::Merge => 4000,      // 4 seconds for merges
        GitOp::Rebase => 6000,     // 6 seconds for rebases (can be complex)
        GitOp::Pull => 5000,       // 5 seconds for pulls
        GitOp::Reset => 3000,      // 3 seconds for resets
        GitOp::Stash => 2000,      // 2 seconds for stash operations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::EventKind;

    #[test]
    fn test_is_git_operation() {
        let git_event = DebouncedEvent {
            paths: vec![PathBuf::from("/project/.git/HEAD")],
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
        };

        let normal_event = DebouncedEvent {
            paths: vec![PathBuf::from("/project/src/main.rs")],
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
        };

        assert!(is_git_operation(&[git_event]));
        assert!(!is_git_operation(&[normal_event]));
    }

    #[test]
    fn test_detect_checkout() {
        let event = DebouncedEvent {
            paths: vec![PathBuf::from("/project/.git/HEAD")],
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
        };

        assert_eq!(detect_git_operation_type(&[event]), Some(GitOp::Checkout));
    }

    #[test]
    fn test_detect_merge() {
        let event = DebouncedEvent {
            paths: vec![PathBuf::from("/project/.git/MERGE_HEAD")],
            kind: EventKind::Create(notify::event::CreateKind::Any),
        };

        assert_eq!(detect_git_operation_type(&[event]), Some(GitOp::Merge));
    }

    #[test]
    fn test_detect_rebase() {
        let event = DebouncedEvent {
            paths: vec![PathBuf::from("/project/.git/rebase-merge/done")],
            kind: EventKind::Create(notify::event::CreateKind::Any),
        };

        assert_eq!(detect_git_operation_type(&[event]), Some(GitOp::Rebase));
    }

    #[test]
    fn test_no_git_operation() {
        let event = DebouncedEvent {
            paths: vec![PathBuf::from("/project/src/lib.rs")],
            kind: EventKind::Modify(notify::event::ModifyKind::Any),
        };

        assert_eq!(detect_git_operation_type(&[event]), None);
    }
}