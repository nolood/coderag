use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::time::{Instant, UNIX_EPOCH};
use tracing::{debug, info};

use crate::embeddings::EmbeddingGenerator;
use crate::indexer::{AstChunker, Chunker, ChunkerStrategy, Walker};
use crate::indexing::ParallelIndexer;
use crate::metrics::{INDEXED_CHUNKS, INDEXED_FILES, INDEX_LATENCY};
use crate::search::bm25::Bm25Search;
use crate::storage::{IndexedChunk, Storage};
use crate::Config;

/// Run the index command
pub async fn run() -> Result<()> {
    run_with_parallel(true).await
}

/// Run the index command with optional parallel processing
pub async fn run_with_parallel(use_parallel: bool) -> Result<()> {
    let root = env::current_dir()?;

    if !Config::is_initialized(&root) {
        bail!("CodeRAG is not initialized. Run 'coderag init' first.");
    }

    let config = Config::load(&root)?;

    // Use parallel indexing if enabled and configured
    if use_parallel && config.indexer.parallel_threads.is_some() {
        return run_parallel_indexing(root, config).await;
    }

    // Start timing for metrics
    let start = Instant::now();

    println!("Starting indexing...");

    // Initialize components
    let storage = Storage::new(&config.db_path(&root)).await?;
    let embedder = EmbeddingGenerator::new(&config.embeddings)?;
    let walker = Walker::new(root.clone(), &config.indexer);

    // Use AST chunker if configured, otherwise use line-based chunker
    let mut ast_chunker: Option<AstChunker> = None;
    let mut line_chunker: Option<Chunker> = None;

    if config.indexer.chunker_strategy == ChunkerStrategy::Ast {
        ast_chunker = Some(AstChunker::with_limits(
            config.indexer.min_chunk_tokens,
            config.indexer.max_chunk_tokens,
        ));
    } else {
        line_chunker = Some(Chunker::new(config.indexer.chunk_size));
    }

    // Get existing file mtimes for incremental indexing
    let existing_mtimes = storage.get_file_mtimes().await?;
    debug!("Found {} files in existing index", existing_mtimes.len());

    // Collect files to process
    let files: Vec<_> = walker.collect_files();
    println!("📁 Found {} files to check", files.len());

    // Filter to files that need indexing (new or modified)
    let files_to_index: Vec<_> = files
        .iter()
        .filter(|path| {
            let current_mtime = get_file_mtime(path).unwrap_or(0);

            if let Some(&stored_mtime) = existing_mtimes.get(*path) {
                current_mtime > stored_mtime
            } else {
                true // New file
            }
        })
        .collect();

    if files_to_index.is_empty() {
        println!("✅ Index is up to date. No files need indexing.");
        return Ok(());
    }

    println!("📝 {} files need indexing", files_to_index.len());

    // Create progress bar
    let pb = ProgressBar::new(files_to_index.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .expect("Invalid progress bar template")
            .progress_chars("#>-"),
    );

    let mut total_chunks = 0;
    let mut all_chunks: Vec<IndexedChunk> = Vec::new();
    let batch_size = config.embeddings.batch_size;

    for (idx, path) in files_to_index.iter().enumerate() {
        pb.set_position(idx as u64);

        // Read file content
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                debug!("Skipping file {:?}: {}", path, e);
                continue;
            }
        };

        // Get file mtime
        let mtime = get_file_mtime(path).unwrap_or(0);

        // Extract file header (first 50 lines)
        let file_header = extract_file_header(&content, 50);

        // Delete existing chunks for this file (for re-indexing)
        storage.delete_by_file(path).await?;

        // Chunk the file using appropriate chunker
        let chunks = if let Some(ref mut ast_chunker) = ast_chunker {
            ast_chunker.chunk_file(path, &content)
        } else if let Some(ref line_chunker) = line_chunker {
            line_chunker.chunk_file(path, &content)
        } else {
            continue;
        };

        if chunks.is_empty() {
            continue;
        }

        // Prepare chunks for embedding
        let chunk_contents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();

        // Generate embeddings
        let embeddings = embedder
            .embed(&chunk_contents)
            .with_context(|| format!("Failed to generate embeddings for {:?}", path))?;

        // Create indexed chunks with symbol metadata
        let file_path_str = path.to_string_lossy().to_string();
        for (chunk, embedding) in chunks.iter().zip(embeddings.into_iter()) {
            all_chunks.push(IndexedChunk {
                id: uuid::Uuid::new_v4().to_string(),
                content: chunk.content.clone(),
                file_path: file_path_str.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                language: chunk.language.clone(),
                vector: embedding,
                mtime,
                file_header: Some(file_header.clone()),
                // Symbol metadata from AST chunking
                semantic_kind: chunk.semantic_kind.map(|k| k.as_str().to_string()),
                symbol_name: chunk.name.clone(),
                signature: chunk.signature.clone(),
                parent: chunk.parent.clone(),
                visibility: None, // Visibility would need to be extracted from AST
            });
        }

        total_chunks += chunks.len();

        // Insert in batches
        if all_chunks.len() >= batch_size * 10 {
            storage.insert_chunks(all_chunks.clone()).await?;
            all_chunks.clear();
        }
    }

    // Insert remaining chunks
    if !all_chunks.is_empty() {
        storage.insert_chunks(all_chunks).await?;
    }

    pb.finish_with_message("done");

    // Record indexing metrics
    let duration = start.elapsed();
    INDEX_LATENCY.observe(duration.as_secs_f64());

    // Print statistics
    let total_chunks_in_db = storage.count_chunks().await?;
    let total_files_in_db = storage.list_files(None).await?.len();

    // Update gauge metrics with current totals
    INDEXED_FILES.set(total_files_in_db as f64);
    INDEXED_CHUNKS.set(total_chunks_in_db as f64);

    println!("\nVector indexing complete!");
    println!("   Files indexed: {}", files_to_index.len());
    println!("   Chunks created: {}", total_chunks);
    println!("   Total in index: {} files, {} chunks", total_files_in_db, total_chunks_in_db);
    println!("   Duration: {:.2}s", duration.as_secs_f64());

    // Build BM25 index for hybrid search
    println!("\nBuilding BM25 index for hybrid search...");
    let bm25_start = Instant::now();

    build_bm25_index(&storage, &Config::coderag_dir(&root)).await?;

    let bm25_duration = bm25_start.elapsed();
    println!("BM25 index built in {:.2}s", bm25_duration.as_secs_f64());

    Ok(())
}

/// Build or rebuild the BM25 index from all chunks in storage.
///
/// This function retrieves all chunks from the vector store and adds them
/// to the Tantivy BM25 index for keyword search.
async fn build_bm25_index(storage: &Storage, coderag_dir: &std::path::Path) -> Result<()> {
    info!("Building BM25 index at {:?}", coderag_dir);

    // Get all chunks from vector storage
    let chunks = storage
        .get_all_chunks()
        .await
        .with_context(|| "Failed to retrieve chunks for BM25 indexing")?;

    if chunks.is_empty() {
        info!("No chunks to index in BM25");
        return Ok(());
    }

    info!("Retrieved {} chunks for BM25 indexing", chunks.len());

    // Create or open the BM25 index
    let bm25 = Bm25Search::new(coderag_dir)
        .with_context(|| "Failed to create BM25 search engine")?;

    // Clear existing index and add all chunks
    {
        let mut index = bm25.index_mut();
        index.clear()?;
        index.add_chunks(&chunks)?;
        index.commit()?;
    }

    info!("BM25 index built with {} chunks", chunks.len());

    Ok(())
}

/// Get the modification time of a file as Unix timestamp
fn get_file_mtime(path: &std::path::Path) -> Result<i64> {
    let metadata = fs::metadata(path)?;
    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(mtime)
}

/// Extract the first N lines of a file as the header for context
fn extract_file_header(content: &str, max_lines: usize) -> String {
    content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run parallel indexing using the ParallelIndexer
async fn run_parallel_indexing(root: std::path::PathBuf, config: Config) -> Result<()> {
    println!("🚀 Starting parallel indexing...");

    // Start timing for metrics
    let start = Instant::now();

    // Initialize parallel indexer
    let indexer = ParallelIndexer::new(root.clone(), config.clone()).await?;

    // Initialize walker to collect files
    let walker = Walker::new(root.clone(), &config.indexer);
    let files: Vec<_> = walker.collect_files();

    println!("📁 Found {} files to check", files.len());

    // Run parallel indexing
    let result = indexer.index_files(files).await?;

    // Print results
    println!("\n{}", result.summary());

    // Record metrics
    let duration = start.elapsed();
    INDEX_LATENCY.observe(duration.as_secs_f64());

    // Get storage stats
    let storage = Storage::new(&config.db_path(&root)).await?;
    let total_chunks_in_db = storage.count_chunks().await?;
    let total_files_in_db = storage.list_files(None).await?.len();

    // Update gauge metrics
    INDEXED_FILES.set(total_files_in_db as f64);
    INDEXED_CHUNKS.set(total_chunks_in_db as f64);

    println!("\n📊 Parallel indexing complete!");
    println!("   Files processed: {}", result.files_processed);
    println!("   Chunks created: {}", result.chunks_created);
    println!("   Total in index: {} files, {} chunks", total_files_in_db, total_chunks_in_db);
    println!("   Duration: {:.2}s", duration.as_secs_f64());
    println!("   Speed: {:.1} files/sec", result.files_processed as f64 / duration.as_secs_f64());

    // Build BM25 index for hybrid search
    println!("\n🔍 Building BM25 index for hybrid search...");
    let bm25_start = Instant::now();

    build_bm25_index(&storage, &Config::coderag_dir(&root)).await?;

    let bm25_duration = bm25_start.elapsed();
    println!("BM25 index built in {:.2}s", bm25_duration.as_secs_f64());

    Ok(())
}
