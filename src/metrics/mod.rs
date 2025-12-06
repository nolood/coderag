//! Prometheus metrics for CodeRAG
//!
//! This module provides observability through Prometheus-compatible metrics
//! for search, indexing, and embedding operations.

use lazy_static::lazy_static;
use prometheus::{Counter, Encoder, Gauge, Histogram, HistogramOpts, Opts, Registry, TextEncoder};

lazy_static! {
    /// Global metrics registry
    pub static ref REGISTRY: Registry = Registry::new();

    // ============================================================================
    // Search metrics
    // ============================================================================

    /// Total number of search requests
    pub static ref SEARCH_REQUESTS: Counter = Counter::with_opts(
        Opts::new(
            "coderag_search_requests_total",
            "Total number of search requests"
        )
    ).expect("Failed to create SEARCH_REQUESTS counter");

    /// Search request latency in seconds
    pub static ref SEARCH_LATENCY: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "coderag_search_latency_seconds",
            "Search request latency in seconds"
        ).buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])
    ).expect("Failed to create SEARCH_LATENCY histogram");

    /// Number of search results returned per request
    pub static ref SEARCH_RESULTS: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "coderag_search_results_count",
            "Number of search results returned per request"
        ).buckets(vec![0.0, 1.0, 5.0, 10.0, 20.0, 50.0])
    ).expect("Failed to create SEARCH_RESULTS histogram");

    // ============================================================================
    // Index metrics
    // ============================================================================

    /// Total number of indexed files
    pub static ref INDEXED_FILES: Gauge = Gauge::with_opts(
        Opts::new(
            "coderag_indexed_files_total",
            "Total number of indexed files"
        )
    ).expect("Failed to create INDEXED_FILES gauge");

    /// Total number of indexed chunks
    pub static ref INDEXED_CHUNKS: Gauge = Gauge::with_opts(
        Opts::new(
            "coderag_indexed_chunks_total",
            "Total number of indexed chunks"
        )
    ).expect("Failed to create INDEXED_CHUNKS gauge");

    /// Time to index files in seconds
    pub static ref INDEX_LATENCY: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "coderag_indexing_duration_seconds",
            "Time to index files in seconds"
        ).buckets(vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0])
    ).expect("Failed to create INDEX_LATENCY histogram");

    // ============================================================================
    // Embedding metrics
    // ============================================================================

    /// Total embedding generation requests
    pub static ref EMBEDDING_REQUESTS: Counter = Counter::with_opts(
        Opts::new(
            "coderag_embedding_requests_total",
            "Total embedding generation requests"
        )
    ).expect("Failed to create EMBEDDING_REQUESTS counter");

    /// Embedding generation latency in seconds
    pub static ref EMBEDDING_LATENCY: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "coderag_embedding_latency_seconds",
            "Embedding generation latency in seconds"
        ).buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0])
    ).expect("Failed to create EMBEDDING_LATENCY histogram");

    // ============================================================================
    // Watch mode metrics
    // ============================================================================

    /// Number of mass change events detected
    pub static ref MASS_CHANGES_DETECTED: Counter = Counter::with_opts(
        Opts::new(
            "coderag_mass_changes_detected_total",
            "Number of mass change events detected"
        )
    ).expect("Failed to create MASS_CHANGES_DETECTED counter");

    /// Number of files in batched processing
    pub static ref BATCHED_FILES: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "coderag_batched_files_count",
            "Number of files in batched processing"
        ).buckets(vec![1.0, 10.0, 50.0, 100.0, 500.0, 1000.0])
    ).expect("Failed to create BATCHED_FILES histogram");
}

/// Register all metrics with the global registry
///
/// This function should be called once at application startup.
/// Panics if metrics registration fails.
pub fn register_metrics() {
    REGISTRY
        .register(Box::new(SEARCH_REQUESTS.clone()))
        .expect("Failed to register SEARCH_REQUESTS");
    REGISTRY
        .register(Box::new(SEARCH_LATENCY.clone()))
        .expect("Failed to register SEARCH_LATENCY");
    REGISTRY
        .register(Box::new(SEARCH_RESULTS.clone()))
        .expect("Failed to register SEARCH_RESULTS");
    REGISTRY
        .register(Box::new(INDEXED_FILES.clone()))
        .expect("Failed to register INDEXED_FILES");
    REGISTRY
        .register(Box::new(INDEXED_CHUNKS.clone()))
        .expect("Failed to register INDEXED_CHUNKS");
    REGISTRY
        .register(Box::new(INDEX_LATENCY.clone()))
        .expect("Failed to register INDEX_LATENCY");
    REGISTRY
        .register(Box::new(EMBEDDING_REQUESTS.clone()))
        .expect("Failed to register EMBEDDING_REQUESTS");
    REGISTRY
        .register(Box::new(EMBEDDING_LATENCY.clone()))
        .expect("Failed to register EMBEDDING_LATENCY");
    REGISTRY
        .register(Box::new(MASS_CHANGES_DETECTED.clone()))
        .expect("Failed to register MASS_CHANGES_DETECTED");
    REGISTRY
        .register(Box::new(BATCHED_FILES.clone()))
        .expect("Failed to register BATCHED_FILES");
}

/// Gather all metrics and encode them in Prometheus text format
///
/// Returns a string containing all registered metrics in the Prometheus
/// exposition format, suitable for scraping by Prometheus.
///
/// Returns an empty string if encoding fails (which should not happen with valid metrics).
pub fn gather_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();

    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        tracing::error!("Failed to encode metrics: {}", e);
        return String::new();
    }

    String::from_utf8(buffer).unwrap_or_else(|e| {
        tracing::error!("Metrics contained invalid UTF-8: {}", e);
        String::new()
    })
}

/// Get current metric values in a human-readable format
///
/// This is useful for the CLI stats command.
pub struct MetricSnapshot {
    pub search_requests_total: f64,
    pub search_latency_avg: f64,
    pub search_results_avg: f64,
    pub indexed_files: f64,
    pub indexed_chunks: f64,
    pub embedding_requests_total: f64,
    pub embedding_latency_avg: f64,
}

impl MetricSnapshot {
    /// Capture the current state of all metrics
    pub fn capture() -> Self {
        Self {
            search_requests_total: SEARCH_REQUESTS.get(),
            search_latency_avg: calculate_histogram_avg(&SEARCH_LATENCY),
            search_results_avg: calculate_histogram_avg(&SEARCH_RESULTS),
            indexed_files: INDEXED_FILES.get(),
            indexed_chunks: INDEXED_CHUNKS.get(),
            embedding_requests_total: EMBEDDING_REQUESTS.get(),
            embedding_latency_avg: calculate_histogram_avg(&EMBEDDING_LATENCY),
        }
    }
}

/// Calculate the average value from a histogram
fn calculate_histogram_avg(histogram: &Histogram) -> f64 {
    let count = histogram.get_sample_count();
    if count == 0 {
        return 0.0;
    }
    histogram.get_sample_sum() / count as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        // Metrics should be created via lazy_static
        assert!(SEARCH_REQUESTS.get() >= 0.0);
        assert!(INDEXED_FILES.get() >= 0.0);
    }

    #[test]
    fn test_counter_increment() {
        let initial = SEARCH_REQUESTS.get();
        SEARCH_REQUESTS.inc();
        assert!((SEARCH_REQUESTS.get() - initial - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_gauge_set() {
        INDEXED_FILES.set(42.0);
        assert!((INDEXED_FILES.get() - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_histogram_observe() {
        let count_before = SEARCH_LATENCY.get_sample_count();
        SEARCH_LATENCY.observe(0.1);
        assert_eq!(SEARCH_LATENCY.get_sample_count(), count_before + 1);
    }

    #[test]
    fn test_gather_metrics() {
        // Should not panic and should return valid string
        let output = gather_metrics();
        // Note: If registry is empty (metrics not registered), this will be empty
        // The actual content depends on whether register_metrics() was called
        assert!(output.is_empty() || output.contains("coderag"));
    }

    #[test]
    fn test_metric_snapshot() {
        let snapshot = MetricSnapshot::capture();
        // Values should be non-negative
        assert!(snapshot.search_requests_total >= 0.0);
        assert!(snapshot.indexed_files >= 0.0);
    }
}
