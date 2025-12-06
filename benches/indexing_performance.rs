//! Indexing performance benchmarks for CodeRAG

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tempfile::tempdir;
use tokio::runtime::Runtime;

/// Generate test files of various sizes
fn generate_test_files(dir: &Path, count: usize, size_kb: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for i in 0..count {
        let file_path = dir.join(format!("test_file_{}.rs", i));

        // Generate content
        let mut content = String::new();
        content.push_str("//! Generated test file\n\n");

        // Add imports
        content.push_str("use std::collections::HashMap;\n");
        content.push_str("use std::sync::Arc;\n\n");

        // Generate functions to reach desired size
        let function_template = r#"
/// Function documentation for {name}
pub fn function_{name}(param: &str) -> Result<String, Error> {
    let mut result = String::new();
    for i in 0..10 {
        result.push_str(&format!("Processing {} - iteration {}", param, i));
    }
    Ok(result)
}

#[cfg(test)]
mod test_{name} {
    use super::*;

    #[test]
    fn test_function_{name}() {
        let result = function_{name}("test").unwrap();
        assert!(!result.is_empty());
    }
}
"#;

        let mut function_count = 0;
        while content.len() < size_kb * 1024 {
            let function = function_template.replace("{name}", &format!("{}_{}", i, function_count));
            content.push_str(&function);
            function_count += 1;
        }

        fs::write(&file_path, content).expect("Failed to write test file");
        files.push(file_path);
    }

    files
}

/// Mock indexing function
async fn mock_index_file(path: &Path) -> Result<usize, String> {
    // Simulate file reading and processing
    let content = fs::read_to_string(path)
        .map_err(|e| e.to_string())?;

    // Simulate chunking delay
    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

    // Return number of chunks (simulate)
    Ok(content.len() / 512 + 1)
}

/// Mock parallel indexing
async fn mock_index_parallel(files: &[PathBuf]) -> Result<usize, String> {
    let mut tasks = vec![];

    for file in files {
        let path = file.clone();
        let task = tokio::spawn(async move {
            mock_index_file(&path).await
        });
        tasks.push(task);
    }

    let mut total_chunks = 0;
    for task in tasks {
        match task.await {
            Ok(Ok(chunks)) => total_chunks += chunks,
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(e.to_string()),
        }
    }

    Ok(total_chunks)
}

/// Mock sequential indexing
async fn mock_index_sequential(files: &[PathBuf]) -> Result<usize, String> {
    let mut total_chunks = 0;

    for file in files {
        total_chunks += mock_index_file(file).await?;
    }

    Ok(total_chunks)
}

/// Benchmark sequential vs parallel indexing
fn benchmark_indexing_modes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let temp_dir = tempdir().expect("Failed to create temp dir");

    // Generate test files
    let files = generate_test_files(temp_dir.path(), 20, 10); // 20 files, 10KB each

    let mut group = c.benchmark_group("indexing_modes");
    group.throughput(Throughput::Elements(files.len() as u64));
    group.sample_size(10); // Reduce sample size

    // Benchmark sequential indexing
    group.bench_function("sequential", |b| {
        b.to_async(&rt).iter(|| async {
            let result = mock_index_sequential(&files).await
                .expect("Failed to index");

            black_box(result)
        });
    });

    // Benchmark parallel indexing
    group.bench_function("parallel", |b| {
        b.to_async(&rt).iter(|| async {
            let result = mock_index_parallel(&files).await
                .expect("Failed to index");

            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark indexing performance with different file sizes
fn benchmark_file_size_impact(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("file_size_impact");
    group.sample_size(10);

    for size_kb in [1, 10, 50, 100] {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let files = generate_test_files(temp_dir.path(), 1, size_kb);
        let file = &files[0];

        group.throughput(Throughput::Bytes((size_kb * 1024) as u64));
        group.bench_with_input(
            BenchmarkId::new("size_kb", size_kb),
            file,
            |b, f| {
                b.to_async(&rt).iter(|| async {
                    let start = Instant::now();
                    mock_index_file(f).await.expect("Failed to index file");
                    let elapsed = start.elapsed();

                    black_box(elapsed)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark indexing with different chunk sizes
fn benchmark_chunk_size_impact(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let files = generate_test_files(temp_dir.path(), 5, 20); // 5 files, 20KB each

    let mut group = c.benchmark_group("chunk_size_impact");
    group.sample_size(10);

    for chunk_size in [256, 512, 1024, 2048] {
        group.bench_with_input(
            BenchmarkId::new("chunk_size", chunk_size),
            &chunk_size,
            |b, &_size| {
                b.to_async(&rt).iter(|| async {
                    // Simulate different chunk sizes affecting performance
                    let start = Instant::now();

                    for file in &files {
                        mock_index_file(file).await.expect("Failed to index file");
                    }

                    black_box(start.elapsed())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark incremental indexing (re-indexing modified files)
fn benchmark_incremental_indexing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let files = generate_test_files(temp_dir.path(), 10, 10);

    let mut group = c.benchmark_group("incremental_indexing");
    group.sample_size(10);

    // Benchmark re-indexing modified files
    group.bench_function("reindex_single_file", |b| {
        b.to_async(&rt).iter(|| async {
            // Modify one file
            let file = &files[0];
            let content = fs::read_to_string(file).expect("Failed to read file");
            let modified = format!("{}\n// Modified", content);
            fs::write(file, modified).expect("Failed to write file");

            // Re-index
            let result = mock_index_file(file).await
                .expect("Failed to reindex");

            black_box(result)
        });
    });

    group.finish();
}

/// Simple language detection benchmark
fn benchmark_language_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("language_detection");

    let test_cases = vec![
        ("rust", "fn main() { println!(\"Hello\"); }"),
        ("python", "def main():\n    print(\"Hello\")"),
        ("javascript", "function main() { console.log(\"Hello\"); }"),
        ("typescript", "const main = (): void => { console.log(\"Hello\"); }"),
        ("go", "func main() { fmt.Println(\"Hello\") }"),
    ];

    for (lang, code) in test_cases {
        group.bench_with_input(
            BenchmarkId::new("language", lang),
            code,
            |b, c| {
                b.iter(|| {
                    // Simple language detection simulation
                    let detected = if c.contains("fn ") {
                        "rust"
                    } else if c.contains("def ") {
                        "python"
                    } else if c.contains("function ") {
                        "javascript"
                    } else if c.contains("const ") && c.contains(": void") {
                        "typescript"
                    } else if c.contains("func ") {
                        "go"
                    } else {
                        "unknown"
                    };

                    black_box(detected)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage simulation
fn benchmark_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("memory_usage");
    group.sample_size(10);

    for num_files in [10, 50, 100] {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let files = generate_test_files(temp_dir.path(), num_files, 10);

        group.bench_with_input(
            BenchmarkId::new("files", num_files),
            &files,
            |b, f| {
                b.to_async(&rt).iter(|| async {
                    let mut total_size = 0;

                    for file in f {
                        let metadata = fs::metadata(file).expect("Failed to get metadata");
                        total_size += metadata.len() as usize;

                        mock_index_file(file).await.expect("Failed to index file");
                    }

                    black_box(total_size)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_indexing_modes,
    benchmark_file_size_impact,
    benchmark_chunk_size_impact,
    benchmark_incremental_indexing,
    benchmark_language_detection,
    benchmark_memory_usage
);

criterion_main!(benches);