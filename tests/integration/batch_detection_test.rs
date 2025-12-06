//! Integration tests for batch detection in watch mode

use coderag::watcher::{
    BatchDetector, ChangeAccumulator, ChangeType, FileChange,
    detect_git_operation_type, is_git_operation, GitDebouncedEvent, GitOp,
};
use notify::EventKind;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::thread;

#[test]
fn test_batch_detector_threshold() {
    let mut detector = BatchDetector::new(50, 20.0, Duration::from_millis(3000));

    // Test threshold-based detection
    assert!(!detector.detect_mass_change(30));
    assert!(!detector.detect_mass_change(49));
    assert!(detector.detect_mass_change(50));
    assert!(detector.detect_mass_change(100));
}

#[test]
fn test_batch_detector_rate() {
    let mut detector = BatchDetector::new(100, 5.0, Duration::from_millis(3000));

    // Simulate rapid changes to trigger rate detection
    for i in 0..6 {
        let detected = detector.detect_mass_change(10);
        // First few shouldn't trigger, but eventually rate should trigger
        if i < 3 {
            assert!(!detected, "Should not detect mass change at iteration {}", i);
        }
        thread::sleep(Duration::from_millis(50));
    }

    // After rapid changes, rate should be high
    let rate = detector.current_rate();
    assert!(rate > 0.0, "Rate should be positive after rapid changes");
}

#[test]
fn test_git_operation_detection() {
    // Test checkout detection
    let checkout_event = GitDebouncedEvent {
        paths: vec![PathBuf::from("/project/.git/HEAD")],
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
    };
    assert!(is_git_operation(&[checkout_event.clone()]));
    assert_eq!(detect_git_operation_type(&[checkout_event]), Some(GitOp::Checkout));

    // Test merge detection
    let merge_event = GitDebouncedEvent {
        paths: vec![PathBuf::from("/project/.git/MERGE_HEAD")],
        kind: EventKind::Create(notify::event::CreateKind::Any),
    };
    assert!(is_git_operation(&[merge_event.clone()]));
    assert_eq!(detect_git_operation_type(&[merge_event]), Some(GitOp::Merge));

    // Test rebase detection
    let rebase_event = GitDebouncedEvent {
        paths: vec![PathBuf::from("/project/.git/rebase-merge/done")],
        kind: EventKind::Create(notify::event::CreateKind::Any),
    };
    assert!(is_git_operation(&[rebase_event.clone()]));
    assert_eq!(detect_git_operation_type(&[rebase_event]), Some(GitOp::Rebase));

    // Test non-git event
    let normal_event = GitDebouncedEvent {
        paths: vec![PathBuf::from("/project/src/main.rs")],
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
    };
    assert!(!is_git_operation(&[normal_event.clone()]));
    assert_eq!(detect_git_operation_type(&[normal_event]), None);
}

#[test]
fn test_change_accumulator() {
    let mut acc = ChangeAccumulator::new();
    let now = Instant::now();

    // Test basic accumulation
    let file1 = FileChange {
        path: PathBuf::from("/test/file1.rs"),
        change_type: ChangeType::Created,
        timestamp: now,
    };
    acc.add_change(file1);
    assert_eq!(acc.len(), 1);
    assert!(acc.is_accumulating());

    // Test deduplication - same file modified twice
    let file1_mod = FileChange {
        path: PathBuf::from("/test/file1.rs"),
        change_type: ChangeType::Modified,
        timestamp: now,
    };
    acc.add_change(file1_mod);
    assert_eq!(acc.len(), 1); // Should still be 1 due to deduplication

    // Test different file
    let file2 = FileChange {
        path: PathBuf::from("/test/file2.rs"),
        change_type: ChangeType::Created,
        timestamp: now,
    };
    acc.add_change(file2);
    assert_eq!(acc.len(), 2);

    // Test flush
    let delay = Duration::from_millis(50);
    thread::sleep(delay);
    assert!(acc.should_flush(delay));

    let flushed = acc.flush();
    assert_eq!(flushed.len(), 2);
    assert!(!acc.is_accumulating());
    assert_eq!(acc.len(), 0);
}

#[test]
fn test_rename_handling() {
    let mut acc = ChangeAccumulator::new();
    let now = Instant::now();

    // Add original file
    let original = FileChange {
        path: PathBuf::from("/test/old_name.rs"),
        change_type: ChangeType::Created,
        timestamp: now,
    };
    acc.add_change(original);
    assert_eq!(acc.len(), 1);

    // Rename the file
    let renamed = FileChange {
        path: PathBuf::from("/test/new_name.rs"),
        change_type: ChangeType::Renamed {
            from: PathBuf::from("/test/old_name.rs"),
        },
        timestamp: now,
    };
    acc.add_change(renamed);

    // Should only have the renamed file, not both
    let flushed = acc.flush();
    assert_eq!(flushed.len(), 1);
    assert_eq!(flushed[0].path, PathBuf::from("/test/new_name.rs"));
}

#[test]
fn test_mass_change_simulation() {
    let mut detector = BatchDetector::new(50, 20.0, Duration::from_millis(3000));
    let mut accumulator = ChangeAccumulator::new();
    let now = Instant::now();

    // Simulate git checkout with many files
    let change_count = 100;
    let mut changes = Vec::new();
    for i in 0..change_count {
        changes.push(FileChange {
            path: PathBuf::from(format!("/project/src/file{}.rs", i)),
            change_type: ChangeType::Modified,
            timestamp: now,
        });
    }

    // Should detect as mass change
    assert!(detector.detect_mass_change(changes.len()));

    // Accumulate all changes
    for change in changes {
        accumulator.add_change(change);
    }

    assert_eq!(accumulator.len(), change_count);

    // Wait for collection delay
    let delay = detector.collection_delay();
    thread::sleep(delay);
    assert!(accumulator.should_flush(delay));

    // Flush and verify
    let batched = accumulator.flush();
    assert_eq!(batched.len(), change_count);
}

#[test]
fn test_gradual_changes() {
    let mut detector = BatchDetector::new(50, 20.0, Duration::from_millis(3000));

    // Simulate gradual changes (should not trigger batching)
    for _ in 0..10 {
        assert!(!detector.detect_mass_change(2)); // 2 files at a time
        thread::sleep(Duration::from_millis(500)); // Slow rate
    }

    // Rate should be low
    let rate = detector.current_rate();
    assert!(rate < 20.0, "Rate should be below threshold for gradual changes");
}