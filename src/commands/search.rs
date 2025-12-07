use anyhow::Result;
use std::env;
use std::sync::Arc;

use crate::auto_index::{AutoIndexPolicy, AutoIndexService};
use crate::embeddings::EmbeddingGenerator;
use crate::search::traits::Search;
use crate::search::SearchEngine;
use crate::storage::Storage;
use crate::Config;

/// Run the search command
///
/// With zero-ceremony support, this command:
/// 1. Auto-detects the project root
/// 2. Resolves storage location (local or global)
/// 3. Auto-indexes if needed (unless `no_auto_index` is set)
/// 4. Performs semantic search
///
/// # Arguments
///
/// * `query` - The search query
/// * `limit` - Maximum number of results to return
/// * `no_auto_index` - Skip auto-indexing before search
pub async fn run(query: &str, limit: Option<usize>, no_auto_index: bool) -> Result<()> {
    let cwd = env::current_dir()?;

    // Set up auto-index service with appropriate policy
    let policy = if no_auto_index {
        AutoIndexPolicy::Never
    } else {
        AutoIndexPolicy::OnMissingOrStale
    };
    let service = AutoIndexService::with_policy(policy);
    let result = service.ensure_indexed(&cwd).await?;

    if result.files_indexed > 0 {
        eprintln!(
            "Indexed {} files ({} chunks) in {:.2}s",
            result.files_indexed, result.chunks_created, result.duration_secs
        );
    }

    // Use resolved storage location for search
    let config = if result.storage.is_local() {
        Config::load(result.storage.root())?
    } else {
        Config::default()
    };

    let limit = limit.unwrap_or(config.search.default_limit);

    // Initialize embedder first to get vector dimension
    let embedder = Arc::new(EmbeddingGenerator::new_async(&config.embeddings).await?);
    let vector_dimension = embedder.embedding_dimension();

    // Initialize storage with vector dimension from embedder
    let storage = Arc::new(Storage::new(result.storage.db_path(), vector_dimension).await?);
    let search_engine = SearchEngine::new(storage, embedder);

    // Perform search
    let results = search_engine.search(query, limit).await?;

    if results.is_empty() {
        println!("No results found for: {}", query);
        println!("\nMake sure you have indexed the codebase with 'coderag index'");
        return Ok(());
    }

    println!("Found {} results for: \"{}\"\n", results.len(), query);

    for (i, result) in results.iter().enumerate() {
        // Format score as percentage
        let score_pct = (result.score * 100.0).round() as i32;

        // Print result header
        println!(
            "{}. {}:{}-{} (score: {}%)",
            i + 1,
            result.file_path,
            result.start_line,
            result.end_line,
            score_pct
        );

        // Print content preview (first few lines)
        let preview = format_preview(&result.content, 5);
        println!("{}", preview);
        println!();
    }

    Ok(())
}

/// Format a preview of the content, limiting to max_lines
fn format_preview(content: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let preview_lines = if lines.len() > max_lines {
        let mut preview: Vec<&str> = lines.iter().take(max_lines).copied().collect();
        preview.push("   ...");
        preview
    } else {
        lines
    };

    preview_lines
        .iter()
        .map(|line| format!("   {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}
