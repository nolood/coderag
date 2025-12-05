use anyhow::{bail, Result};
use std::env;
use std::sync::Arc;

use crate::embeddings::EmbeddingGenerator;
use crate::search::traits::Search;
use crate::search::SearchEngine;
use crate::storage::Storage;
use crate::Config;

/// Run the search command
pub async fn run(query: &str, limit: Option<usize>) -> Result<()> {
    let root = env::current_dir()?;

    if !Config::is_initialized(&root) {
        bail!("CodeRAG is not initialized. Run 'coderag init' first.");
    }

    let config = Config::load(&root)?;
    let limit = limit.unwrap_or(10);

    // Initialize components
    let storage = Arc::new(Storage::new(&config.db_path(&root)).await?);
    let embedder = Arc::new(EmbeddingGenerator::new(&config.embeddings)?);
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
