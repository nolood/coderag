//! Stats command for displaying index statistics and metrics

use anyhow::{bail, Result};
use std::env;

use crate::metrics::{gather_metrics, MetricSnapshot, INDEXED_CHUNKS, INDEXED_FILES};
use crate::storage::Storage;
use crate::Config;

/// Run the stats command
///
/// Displays current index statistics and metrics.
///
/// # Arguments
/// * `prometheus` - If true, output in Prometheus text format
pub async fn run(prometheus: bool) -> Result<()> {
    if prometheus {
        return run_prometheus().await;
    }
    run_human_readable().await
}

/// Run the stats command with human-readable output
async fn run_human_readable() -> Result<()> {
    let root = env::current_dir()?;

    if !Config::is_initialized(&root) {
        bail!("CodeRAG is not initialized. Run 'coderag init' first.");
    }

    let config = Config::load(&root)?;

    // Load storage to get current index statistics
    // Note: For stats, we don't need the exact vector dimension as we're only reading metadata
    let storage = Storage::new_with_default_dimension(&config.db_path(&root)).await?;

    // Get current index stats from storage
    let total_chunks = storage.count_chunks().await?;
    let files = storage.list_files(None).await?;
    let total_files = files.len();

    // Update gauge metrics with current values from storage
    INDEXED_FILES.set(total_files as f64);
    INDEXED_CHUNKS.set(total_chunks as f64);

    // Capture metric snapshot
    let snapshot = MetricSnapshot::capture();

    println!("CodeRAG Index Statistics");
    println!("========================\n");

    println!("Index Contents:");
    println!("  Total files:  {}", total_files);
    println!("  Total chunks: {}", total_chunks);
    println!();

    println!("Search Metrics:");
    println!(
        "  Total requests:   {:.0}",
        snapshot.search_requests_total
    );
    if snapshot.search_requests_total > 0.0 {
        println!(
            "  Average latency:  {:.3}s",
            snapshot.search_latency_avg
        );
        println!(
            "  Average results:  {:.1}",
            snapshot.search_results_avg
        );
    }
    println!();

    println!("Embedding Metrics:");
    println!(
        "  Total requests:   {:.0}",
        snapshot.embedding_requests_total
    );
    if snapshot.embedding_requests_total > 0.0 {
        println!(
            "  Average latency:  {:.3}s",
            snapshot.embedding_latency_avg
        );
    }
    println!();

    // Show database path
    println!("Storage:");
    println!("  Database path: {}", config.db_path(&root).display());

    Ok(())
}

/// Run the stats command with Prometheus format output
///
/// Outputs all metrics in Prometheus text exposition format,
/// suitable for scraping by Prometheus or other monitoring tools.
pub async fn run_prometheus() -> Result<()> {
    let root = env::current_dir()?;

    if !Config::is_initialized(&root) {
        bail!("CodeRAG is not initialized. Run 'coderag init' first.");
    }

    let config = Config::load(&root)?;

    // Load storage to update gauge metrics with current values
    // Note: For stats, we don't need the exact vector dimension as we're only reading metadata
    let storage = Storage::new_with_default_dimension(&config.db_path(&root)).await?;
    let total_chunks = storage.count_chunks().await?;
    let total_files = storage.list_files(None).await?.len();

    // Update gauge metrics
    INDEXED_FILES.set(total_files as f64);
    INDEXED_CHUNKS.set(total_chunks as f64);

    // Output Prometheus format
    let metrics = gather_metrics();
    print!("{}", metrics);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_snapshot_creation() {
        let snapshot = MetricSnapshot::capture();
        // Basic sanity checks - values should be non-negative
        assert!(snapshot.search_requests_total >= 0.0);
        assert!(snapshot.indexed_files >= 0.0);
        assert!(snapshot.indexed_chunks >= 0.0);
        assert!(snapshot.embedding_requests_total >= 0.0);
    }
}
