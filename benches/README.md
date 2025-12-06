# CodeRAG Benchmarks

Comprehensive benchmark suite for measuring search quality and performance of CodeRAG.

## Benchmark Categories

### 1. Search Quality (`search_quality.rs`)

Measures the quality and accuracy of search results:

- **Precision/Recall**: Evaluates how well the search finds relevant code
- **F1 Score**: Harmonic mean of precision and recall
- **MRR (Mean Reciprocal Rank)**: Position of first relevant result
- **NDCG (Normalized Discounted Cumulative Gain)**: Quality of result ranking
- **Search Modes**: Compares Vector, BM25, and Hybrid search modes
- **Query Complexity**: Impact of query complexity on performance

#### Quality Metrics

- **Precision**: `relevant_results / total_results`
- **Recall**: `found_relevant / total_relevant`
- **F1 Score**: `2 * (precision * recall) / (precision + recall)`
- **MRR**: `1 / position_of_first_relevant`
- **NDCG**: Normalized cumulative gain with position discount

### 2. Indexing Performance (`indexing_performance.rs`)

Measures indexing speed and resource usage:

- **Sequential vs Parallel**: Comparison of indexing modes
- **File Size Impact**: Performance with different file sizes
- **Chunk Size Impact**: Effect of chunk size on performance
- **Incremental Indexing**: Re-indexing performance
- **Language Detection**: Speed of language identification
- **Memory Usage**: Memory consumption during indexing

## Running Benchmarks

### Run All Benchmarks

```bash
cargo bench
```

### Run Specific Benchmark

```bash
# Search quality only
cargo bench --bench search_quality

# Indexing performance only
cargo bench --bench indexing_performance
```

### Generate HTML Reports

```bash
# Generates detailed HTML reports in target/criterion/
cargo bench --bench search_quality -- --verbose
```

### Run with Baseline Comparison

```bash
# Save baseline
cargo bench --bench search_quality -- --save-baseline main

# Compare against baseline
cargo bench --bench search_quality -- --baseline main
```

## Test Data

### Query Dataset (`fixtures/queries.json`)

Contains 10 benchmark queries testing different search scenarios:
- Algorithm implementations (fibonacci)
- Error handling patterns
- Async operations
- API endpoints
- Database operations
- String parsing
- Authentication
- Configuration management

Each query includes:
- Query text
- Expected files to find
- Expected symbols
- Minimum precision threshold
- Minimum recall threshold

### Sample Codebase (`fixtures/test_codebase/`)

10 Rust files covering common programming patterns:
- `fibonacci.rs`: Math algorithms
- `error.rs`: Error handling
- `async_handler.rs`: Async patterns
- `api.rs`: HTTP endpoints
- `database.rs`: Database operations
- `parser.rs`: String parsing
- `auth.rs`: Authentication
- `config.rs`: Configuration
- And more...

## Benchmark Results

### Expected Performance Targets

#### Search Quality
- Average Precision: > 75%
- Average Recall: > 70%
- Average F1 Score: > 72%
- Average MRR: > 0.7
- Search Latency: < 50ms for 10 results

#### Indexing Performance
- Sequential: ~100 files/second
- Parallel: ~300 files/second
- Throughput: > 10 MB/second
- Memory: < 100 MB for 100 files

## Continuous Integration

Benchmarks are run in CI to track performance regressions:

```yaml
# .github/workflows/benchmark.yml
- name: Run benchmarks
  run: cargo bench --bench search_quality

- name: Upload results
  uses: benchmark-action/github-action-benchmark@v1
  with:
    tool: 'cargo'
    output-file-path: target/criterion/search_quality/base/estimates.json
    github-token: ${{ secrets.GITHUB_TOKEN }}
    auto-push: true
```

## Analyzing Results

### View Detailed Reports

After running benchmarks, open the HTML reports:

```bash
# View in browser
open target/criterion/search_quality/report/index.html
open target/criterion/indexing_performance/report/index.html
```

### Key Metrics to Monitor

1. **Search Quality Regression**: Precision/Recall drops below thresholds
2. **Performance Degradation**: Latency increases > 20%
3. **Memory Leaks**: Memory usage grows linearly with file count
4. **Mode Comparison**: Hybrid search should outperform single modes

## Adding New Benchmarks

To add a new benchmark query:

1. Edit `fixtures/queries.json`
2. Add test files to `fixtures/test_codebase/`
3. Run benchmarks to establish baseline
4. Update minimum thresholds based on results

## Troubleshooting

### Benchmarks Fail Quality Thresholds

- Check if test files are properly indexed
- Verify embeddings are generated correctly
- Review query-file mappings in queries.json

### Performance Issues

- Ensure release mode: `cargo bench` (not `cargo bench --debug`)
- Check system resources during benchmarking
- Disable other applications to reduce noise

### Inconsistent Results

- Increase sample size in benchmark groups
- Run with `--warm-up` flag
- Use `--save-baseline` for comparisons