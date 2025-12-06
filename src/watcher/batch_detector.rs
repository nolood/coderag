use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tracing::info;

/// Detects mass file changes using threshold and rate-based detection
pub struct BatchDetector {
    /// Minimum number of files to trigger mass change detection
    threshold_files: usize,
    /// Minimum rate of changes per second to trigger detection
    threshold_rate: f64,
    /// Delay to collect changes during mass operations
    collection_delay: Duration,
    /// Recent changes with timestamps for rate calculation
    recent_changes: VecDeque<Instant>,
}

impl BatchDetector {
    pub fn new(threshold_files: usize, threshold_rate: f64, collection_delay: Duration) -> Self {
        Self {
            threshold_files,
            threshold_rate,
            collection_delay,
            recent_changes: VecDeque::new(),
        }
    }

    /// Detects if current changes constitute a mass change event
    pub fn detect_mass_change(&mut self, change_count: usize) -> bool {
        // Threshold-based detection - immediate trigger for large batches
        if change_count >= self.threshold_files {
            info!(
                "Mass change detected via threshold: {} files >= {} threshold",
                change_count, self.threshold_files
            );
            return true;
        }

        // Rate-based detection - track change rate over time
        let now = Instant::now();

        // Add current timestamp for each change
        for _ in 0..change_count {
            self.recent_changes.push_back(now);
        }

        // Remove old entries (older than 10 seconds)
        let cutoff = now - Duration::from_secs(10);
        while let Some(&first) = self.recent_changes.front() {
            if first < cutoff {
                self.recent_changes.pop_front();
            } else {
                break;
            }
        }

        // Calculate rate over the last 10 seconds
        let rate = self.recent_changes.len() as f64 / 10.0;
        if rate >= self.threshold_rate {
            info!(
                "Mass change detected via rate: {:.2} changes/sec >= {:.2} threshold",
                rate, self.threshold_rate
            );
            true
        } else {
            false
        }
    }

    /// Returns the configured collection delay for batching
    pub fn collection_delay(&self) -> Duration {
        self.collection_delay
    }

    /// Clears the recent changes history
    pub fn reset(&mut self) {
        self.recent_changes.clear();
    }

    /// Returns current change rate (changes per second)
    pub fn current_rate(&self) -> f64 {
        if self.recent_changes.is_empty() {
            return 0.0;
        }

        let now = Instant::now();
        let cutoff = now - Duration::from_secs(10);
        let recent_count = self.recent_changes
            .iter()
            .filter(|&&t| t >= cutoff)
            .count();

        recent_count as f64 / 10.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_threshold_detection() {
        let mut detector = BatchDetector::new(50, 20.0, Duration::from_millis(3000));

        // Below threshold
        assert!(!detector.detect_mass_change(30));
        assert!(!detector.detect_mass_change(49));

        // At or above threshold
        assert!(detector.detect_mass_change(50));
        assert!(detector.detect_mass_change(100));
    }

    #[test]
    fn test_rate_detection() {
        let mut detector = BatchDetector::new(100, 5.0, Duration::from_millis(3000));

        // Simulate rapid changes (should trigger rate detection)
        for _ in 0..6 {
            assert!(!detector.detect_mass_change(10)); // 10 files each time
            thread::sleep(Duration::from_millis(100));
        }

        // After 60 changes in ~0.6 seconds, rate should be high
        let rate = detector.current_rate();
        assert!(rate >= 5.0);
    }

    #[test]
    fn test_collection_delay() {
        let detector = BatchDetector::new(50, 20.0, Duration::from_millis(3000));
        assert_eq!(detector.collection_delay(), Duration::from_millis(3000));
    }

    #[test]
    fn test_reset() {
        let mut detector = BatchDetector::new(50, 20.0, Duration::from_millis(3000));

        detector.detect_mass_change(30);
        assert!(detector.recent_changes.len() > 0);

        detector.reset();
        assert_eq!(detector.recent_changes.len(), 0);
        assert_eq!(detector.current_rate(), 0.0);
    }
}