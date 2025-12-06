//! Search quality benchmarks for CodeRAG

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::tempdir;
use tokio::runtime::Runtime;

/// Benchmark query structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkQuery {
    id: String,
    query: String,
    expected_files: Vec<String>,
    expected_symbols: Vec<String>,
    min_precision: f64,
    min_recall: f64,
}

/// Collection of benchmark queries
#[derive(Debug, Serialize, Deserialize)]
struct BenchmarkQueries {
    queries: Vec<BenchmarkQuery>,
}

/// Search quality metrics
#[derive(Debug, Clone)]
struct SearchQualityMetrics {
    precision: f64,
    recall: f64,
    f1_score: f64,
    mrr: f64, // Mean Reciprocal Rank
    ndcg: f64, // Normalized Discounted Cumulative Gain
    latency_ms: f64,
    results_count: usize,
}

impl SearchQualityMetrics {
    /// Calculate F1 score from precision and recall
    fn calculate_f1(&mut self) {
        if self.precision + self.recall > 0.0 {
            self.f1_score = 2.0 * (self.precision * self.recall) / (self.precision + self.recall);
        } else {
            self.f1_score = 0.0;
        }
    }
}

/// Simple mock search result for benchmarking
#[derive(Debug, Clone)]
struct MockSearchResult {
    pub file_path: String,
    pub score: f32,
    pub content: String,
}

/// Load benchmark queries from JSON file
fn load_queries() -> BenchmarkQueries {
    let path = PathBuf::from("benches/fixtures/queries.json");

    // If file doesn't exist, create default queries
    if !path.exists() {
        return BenchmarkQueries {
            queries: vec![
                BenchmarkQuery {
                    id: "test1".to_string(),
                    query: "fibonacci function".to_string(),
                    expected_files: vec!["fibonacci.rs".to_string()],
                    expected_symbols: vec!["fibonacci".to_string()],
                    min_precision: 0.8,
                    min_recall: 0.7,
                },
            ],
        };
    }

    let contents = fs::read_to_string(&path)
        .expect("Failed to read queries.json");

    serde_json::from_str(&contents)
        .expect("Failed to parse queries.json")
}

/// Mock search function for benchmarking
async fn mock_search(query: &str, _limit: usize) -> Vec<MockSearchResult> {
    // Simulate search with some delay
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Return mock results based on query
    if query.contains("fibonacci") {
        vec![
            MockSearchResult {
                file_path: "fibonacci.rs".to_string(),
                score: 0.95,
                content: "fibonacci function implementation".to_string(),
            },
            MockSearchResult {
                file_path: "math.rs".to_string(),
                score: 0.75,
                content: "math utilities".to_string(),
            },
        ]
    } else if query.contains("error") {
        vec![
            MockSearchResult {
                file_path: "error.rs".to_string(),
                score: 0.90,
                content: "error handling".to_string(),
            },
        ]
    } else {
        vec![
            MockSearchResult {
                file_path: "generic.rs".to_string(),
                score: 0.50,
                content: "generic result".to_string(),
            },
        ]
    }
}

/// Calculate precision: relevant results / total results
fn calculate_precision(results: &[MockSearchResult], expected: &[String]) -> f64 {
    if results.is_empty() {
        return 0.0;
    }

    let expected_set: HashSet<String> = expected.iter().cloned().collect();
    let relevant_count = results
        .iter()
        .filter(|r| {
            let file_name = Path::new(&r.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            expected_set.contains(file_name)
        })
        .count();

    relevant_count as f64 / results.len() as f64
}

/// Calculate recall: found relevant / total relevant
fn calculate_recall(results: &[MockSearchResult], expected: &[String]) -> f64 {
    if expected.is_empty() {
        return 1.0;
    }

    let result_files: HashSet<String> = results
        .iter()
        .filter_map(|r| {
            Path::new(&r.file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .collect();

    let found_count = expected
        .iter()
        .filter(|e| result_files.contains(*e))
        .count();

    found_count as f64 / expected.len() as f64
}

/// Calculate Mean Reciprocal Rank
fn calculate_mrr(results: &[MockSearchResult], expected: &[String]) -> f64 {
    let expected_set: HashSet<String> = expected.iter().cloned().collect();

    for (i, result) in results.iter().enumerate() {
        let file_name = Path::new(&result.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if expected_set.contains(file_name) {
            return 1.0 / (i + 1) as f64;
        }
    }

    0.0
}

/// Calculate Normalized Discounted Cumulative Gain
fn calculate_ndcg(results: &[MockSearchResult], expected: &[String]) -> f64 {
    if results.is_empty() || expected.is_empty() {
        return 0.0;
    }

    let expected_set: HashSet<String> = expected.iter().cloned().collect();

    // Calculate DCG
    let mut dcg = 0.0;
    for (i, result) in results.iter().enumerate() {
        let file_name = Path::new(&result.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let relevance = if expected_set.contains(file_name) {
            1.0
        } else {
            0.0
        };

        if i == 0 {
            dcg += relevance;
        } else {
            dcg += relevance / ((i + 1) as f64).log2();
        }
    }

    // Calculate IDCG (ideal DCG)
    let ideal_results = expected.len().min(results.len());
    let mut idcg = 0.0;
    for i in 0..ideal_results {
        if i == 0 {
            idcg += 1.0;
        } else {
            idcg += 1.0 / ((i + 1) as f64).log2();
        }
    }

    if idcg > 0.0 {
        dcg / idcg
    } else {
        0.0
    }
}

/// Benchmark search quality with different queries
fn benchmark_search_quality(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Load benchmark queries
    let queries = load_queries();

    let mut group = c.benchmark_group("search_quality");
    group.sample_size(20); // Reduce sample size for faster benchmarking

    for query in &queries.queries {
        group.bench_with_input(
            BenchmarkId::new("precision_recall", &query.id),
            query,
            |b, q| {
                b.to_async(&rt).iter(|| async {
                    let start = Instant::now();

                    // Perform mock search
                    let results = mock_search(&q.query, 10).await;

                    let latency = start.elapsed();

                    // Calculate metrics
                    let precision = calculate_precision(&results, &q.expected_files);
                    let recall = calculate_recall(&results, &q.expected_files);
                    let mrr = calculate_mrr(&results, &q.expected_files);
                    let ndcg = calculate_ndcg(&results, &q.expected_files);

                    let mut metrics = SearchQualityMetrics {
                        precision,
                        recall,
                        f1_score: 0.0,
                        mrr,
                        ndcg,
                        latency_ms: latency.as_micros() as f64 / 1000.0,
                        results_count: results.len(),
                    };

                    metrics.calculate_f1();

                    black_box(metrics)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark different search modes (simulated)
fn benchmark_search_modes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("search_modes");
    group.sample_size(20);

    let test_query = "error handling with Result type";

    for mode in ["vector", "bm25", "hybrid"] {
        group.bench_with_input(
            BenchmarkId::new("mode", mode),
            &mode,
            |b, _m| {
                b.to_async(&rt).iter(|| async {
                    let start = Instant::now();

                    // Perform mock search
                    let results = mock_search(&test_query, 10).await;

                    let latency = start.elapsed();

                    black_box((results.len(), latency))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark search latency with different result limits
fn benchmark_search_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("search_latency");

    let test_query = "function to calculate fibonacci";

    for limit in [5, 10, 20, 50] {
        group.bench_with_input(
            BenchmarkId::new("limit", limit),
            &limit,
            |b, &l| {
                b.to_async(&rt).iter(|| async {
                    let start = Instant::now();

                    let _results = mock_search(&test_query, l).await;

                    black_box(start.elapsed())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark query complexity impact
fn benchmark_query_complexity(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("query_complexity");
    group.sample_size(20);

    let queries = vec![
        ("simple", "fibonacci"),
        ("medium", "async function with error handling"),
        ("complex", "database connection pool with transaction support and error recovery"),
    ];

    for (complexity, query) in queries {
        group.bench_with_input(
            BenchmarkId::new("complexity", complexity),
            &query,
            |b, q| {
                b.to_async(&rt).iter(|| async {
                    let start = Instant::now();

                    let results = mock_search(q, 10).await;

                    let latency = start.elapsed();

                    (results.len(), latency)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_search_quality,
    benchmark_search_modes,
    benchmark_search_latency,
    benchmark_query_complexity
);

criterion_main!(benches);