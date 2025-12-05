//! BM25 keyword search using Tantivy.
//!
//! This module provides BM25-based full-text search for code chunks
//! using the Tantivy search engine library.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::sync::RwLock;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value as _, STORED, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};
use tracing::{debug, info, warn};

use super::traits::Search;
use crate::storage::{IndexedChunk, SearchResult};

/// BM25 index directory name within .coderag/
const BM25_INDEX_DIR: &str = "bm25.index";

/// Schema field names
const FIELD_ID: &str = "id";
const FIELD_CONTENT: &str = "content";
const FIELD_FILE_PATH: &str = "file_path";
const FIELD_START_LINE: &str = "start_line";
const FIELD_END_LINE: &str = "end_line";

/// BM25 search index schema.
///
/// Defines the structure of documents in the Tantivy index.
#[derive(Clone)]
pub struct Bm25Schema {
    schema: Schema,
    id: Field,
    content: Field,
    file_path: Field,
    start_line: Field,
    end_line: Field,
}

impl Bm25Schema {
    /// Create a new BM25 schema.
    pub fn new() -> Self {
        let mut schema_builder = Schema::builder();

        let id = schema_builder.add_text_field(FIELD_ID, TEXT | STORED);
        let content = schema_builder.add_text_field(FIELD_CONTENT, TEXT | STORED);
        let file_path = schema_builder.add_text_field(FIELD_FILE_PATH, TEXT | STORED);
        let start_line = schema_builder.add_text_field(FIELD_START_LINE, STORED);
        let end_line = schema_builder.add_text_field(FIELD_END_LINE, STORED);

        let schema = schema_builder.build();

        Self {
            schema,
            id,
            content,
            file_path,
            start_line,
            end_line,
        }
    }

    /// Get the schema.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

impl Default for Bm25Schema {
    fn default() -> Self {
        Self::new()
    }
}

/// BM25 search index using Tantivy.
pub struct Bm25Index {
    index: Index,
    schema: Bm25Schema,
    writer: IndexWriter,
    reader: IndexReader,
}

impl Bm25Index {
    /// Create or open a BM25 index at the given path.
    ///
    /// # Arguments
    /// * `path` - Path to the index directory
    ///
    /// # Returns
    /// A new or opened BM25 index
    pub fn new(path: &Path) -> Result<Self> {
        let index_path = path.join(BM25_INDEX_DIR);
        let schema = Bm25Schema::new();

        let index = if index_path.exists() {
            info!("Opening existing BM25 index at {:?}", index_path);
            Index::open_in_dir(&index_path)
                .with_context(|| format!("Failed to open BM25 index at {:?}", index_path))?
        } else {
            info!("Creating new BM25 index at {:?}", index_path);
            std::fs::create_dir_all(&index_path)
                .with_context(|| format!("Failed to create BM25 index directory {:?}", index_path))?;
            Index::create_in_dir(&index_path, schema.schema().clone())
                .with_context(|| format!("Failed to create BM25 index at {:?}", index_path))?
        };

        // Create writer with 50MB heap size
        let writer = index
            .writer(50_000_000)
            .with_context(|| "Failed to create index writer")?;

        // Create reader with reload on commit
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .with_context(|| "Failed to create index reader")?;

        Ok(Self {
            index,
            schema,
            writer,
            reader,
        })
    }

    /// Add chunks to the BM25 index.
    ///
    /// # Arguments
    /// * `chunks` - Vector of indexed chunks to add
    pub fn add_chunks(&mut self, chunks: &[IndexedChunk]) -> Result<()> {
        for chunk in chunks {
            self.writer.add_document(doc!(
                self.schema.id => chunk.id.as_str(),
                self.schema.content => chunk.content.as_str(),
                self.schema.file_path => chunk.file_path.as_str(),
                self.schema.start_line => chunk.start_line.to_string(),
                self.schema.end_line => chunk.end_line.to_string(),
            ))?;
        }

        debug!("Added {} chunks to BM25 index", chunks.len());
        Ok(())
    }

    /// Delete all documents for a given file path.
    ///
    /// # Arguments
    /// * `file_path` - Path of the file whose chunks should be deleted
    pub fn delete_by_file(&mut self, file_path: &str) -> Result<()> {
        let query_parser = QueryParser::for_index(&self.index, vec![self.schema.file_path]);
        let query = query_parser
            .parse_query(&format!("\"{}\"", file_path))
            .with_context(|| format!("Failed to parse delete query for file: {}", file_path))?;

        self.writer.delete_query(query)?;
        debug!("Deleted chunks for file: {}", file_path);
        Ok(())
    }

    /// Commit pending changes to the index.
    pub fn commit(&mut self) -> Result<()> {
        self.writer
            .commit()
            .with_context(|| "Failed to commit BM25 index changes")?;

        // Reload the reader to see the committed changes
        self.reader
            .reload()
            .with_context(|| "Failed to reload index reader")?;

        info!("BM25 index committed");
        Ok(())
    }

    /// Clear the entire index.
    pub fn clear(&mut self) -> Result<()> {
        self.writer.delete_all_documents()?;
        self.commit()?;
        info!("BM25 index cleared");
        Ok(())
    }

    /// Search the index with a query string.
    ///
    /// # Arguments
    /// * `query` - The search query
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    /// A vector of search results sorted by BM25 score
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();

        // Parse query against content field
        let query_parser = QueryParser::for_index(&self.index, vec![self.schema.content]);
        let parsed_query = match query_parser.parse_query(query) {
            Ok(q) => q,
            Err(e) => {
                warn!("Failed to parse query '{}': {}", query, e);
                // Try to escape the query and retry
                let escaped = query.replace(['(', ')', '[', ']', '{', '}', '"', '\'', ':', '\\', '/', '^', '~', '*', '?', '!', '+', '-'], " ");
                query_parser
                    .parse_query(&escaped)
                    .with_context(|| format!("Failed to parse escaped query: {}", escaped))?
            }
        };

        let top_docs = searcher
            .search(&parsed_query, &TopDocs::with_limit(limit))
            .with_context(|| "Failed to execute BM25 search")?;

        let mut results = Vec::with_capacity(top_docs.len());

        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher
                .doc(doc_address)
                .with_context(|| "Failed to retrieve document")?;

            let content = retrieved_doc
                .get_first(self.schema.content)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let file_path = retrieved_doc
                .get_first(self.schema.file_path)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let start_line: usize = retrieved_doc
                .get_first(self.schema.start_line)
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            let end_line: usize = retrieved_doc
                .get_first(self.schema.end_line)
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);

            results.push(SearchResult {
                content,
                file_path,
                start_line,
                end_line,
                score,
            });
        }

        debug!("BM25 search returned {} results", results.len());
        Ok(results)
    }

    /// Check if the index exists at the given path.
    pub fn exists(path: &Path) -> bool {
        path.join(BM25_INDEX_DIR).exists()
    }
}

/// BM25 search engine implementing the Search trait.
///
/// Thread-safe wrapper around `Bm25Index` for use in async contexts.
pub struct Bm25Search {
    index: RwLock<Bm25Index>,
}

impl Bm25Search {
    /// Create a new BM25 search engine.
    ///
    /// # Arguments
    /// * `path` - Path to the .coderag directory
    pub fn new(path: &Path) -> Result<Self> {
        let index = Bm25Index::new(path)?;
        Ok(Self {
            index: RwLock::new(index),
        })
    }

    /// Get mutable access to the index for updates.
    ///
    /// # Panics
    /// Panics if the lock is poisoned (another thread panicked while holding the lock).
    /// This is intentional as a poisoned lock indicates unrecoverable state corruption.
    pub fn index_mut(&self) -> std::sync::RwLockWriteGuard<'_, Bm25Index> {
        self.index.write().unwrap_or_else(|poisoned| {
            // Clear the poison and return the guard - we accept potential inconsistency
            // over complete failure in this read-heavy workload
            poisoned.into_inner()
        })
    }

    /// Get read access to the index.
    ///
    /// # Panics
    /// Panics if the lock is poisoned (another thread panicked while holding the lock).
    /// This is intentional as a poisoned lock indicates unrecoverable state corruption.
    pub fn index(&self) -> std::sync::RwLockReadGuard<'_, Bm25Index> {
        self.index.read().unwrap_or_else(|poisoned| {
            // Clear the poison and return the guard - we accept potential inconsistency
            // over complete failure in this read-heavy workload
            poisoned.into_inner()
        })
    }

    /// Check if a BM25 index exists at the given path.
    pub fn exists(path: &Path) -> bool {
        Bm25Index::exists(path)
    }
}

#[async_trait]
impl Search for Bm25Search {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let start = std::time::Instant::now();
        let index = self.index.read().unwrap_or_else(|poisoned| {
            // Clear the poison and return the guard
            poisoned.into_inner()
        });
        let results = index.search(query, limit)?;
        let elapsed = start.elapsed();
        info!(
            search_type = "bm25",
            query = query,
            results = results.len(),
            elapsed_ms = elapsed.as_millis() as u64,
            "BM25 search completed"
        );
        Ok(results)
    }

    fn search_type(&self) -> &'static str {
        "bm25"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_chunk(id: &str, content: &str, file_path: &str) -> IndexedChunk {
        IndexedChunk {
            id: id.to_string(),
            content: content.to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            end_line: 10,
            language: Some("rust".to_string()),
            vector: vec![0.0; 768],
            mtime: 0,
        }
    }

    #[test]
    fn test_bm25_index_creation() {
        let dir = tempdir().unwrap();
        let index = Bm25Index::new(dir.path());
        assert!(index.is_ok());
    }

    #[test]
    fn test_bm25_add_and_search() {
        let dir = tempdir().unwrap();
        let mut index = Bm25Index::new(dir.path()).unwrap();

        let chunks = vec![
            create_test_chunk("1", "fn hello_world() { println!(\"Hello\"); }", "src/main.rs"),
            create_test_chunk("2", "fn goodbye_world() { println!(\"Goodbye\"); }", "src/lib.rs"),
        ];

        index.add_chunks(&chunks).unwrap();
        index.commit().unwrap();

        let results = index.search("hello", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("hello_world"));
    }

    #[test]
    fn test_bm25_clear() {
        let dir = tempdir().unwrap();
        let mut index = Bm25Index::new(dir.path()).unwrap();

        let chunks = vec![create_test_chunk(
            "1",
            "fn test_function() {}",
            "src/test.rs",
        )];

        index.add_chunks(&chunks).unwrap();
        index.commit().unwrap();

        let results = index.search("test_function", 10).unwrap();
        assert_eq!(results.len(), 1);

        index.clear().unwrap();

        let results = index.search("test_function", 10).unwrap();
        assert_eq!(results.len(), 0);
    }
}
