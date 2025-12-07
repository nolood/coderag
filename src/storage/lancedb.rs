use anyhow::{Context, Result};
use arrow_array::types::Float32Type;
use arrow_array::{
    Array, FixedSizeListArray, Int32Array, Int64Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, Connection, Table};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

const TABLE_NAME: &str = "chunks";
const VECTOR_DIMENSION: i32 = 768; // nomic-embed-text-v1.5 dimension
/// Maximum number of rows to query when fetching all data.
/// Used as a fallback when count_rows fails, and as an upper bound for safety.
const MAX_QUERY_ROWS: usize = 10_000_000;

/// Represents an indexed code chunk ready for storage
#[derive(Debug, Clone)]
pub struct IndexedChunk {
    pub id: String,
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub language: Option<String>,
    pub vector: Vec<f32>,
    pub mtime: i64,
    /// First 50 lines of the file for context
    pub file_header: Option<String>,
    /// Semantic kind of the chunk (function, class, struct, etc.)
    pub semantic_kind: Option<String>,
    /// Name of the symbol (e.g., function name, class name)
    pub symbol_name: Option<String>,
    /// Full signature of the symbol (e.g., function signature with parameters)
    pub signature: Option<String>,
    /// Parent symbol context (e.g., class name for methods)
    pub parent: Option<String>,
    /// Visibility modifier (public, private, protected)
    pub visibility: Option<String>,
}

/// Search result from vector similarity search
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub content: String,
    pub file_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f32,
    /// First 50 lines of the file for context
    pub file_header: Option<String>,
}

/// LanceDB storage backend for vector embeddings
pub struct Storage {
    db: Connection,
    db_path: PathBuf,
}

impl Storage {
    /// Create or open a LanceDB storage at the given path
    pub async fn new(path: &Path) -> Result<Self> {
        let db_path = path.to_path_buf();
        let path_str = path.to_string_lossy();

        info!("Opening LanceDB at: {}", path_str);

        let db = connect(&path_str)
            .execute()
            .await
            .with_context(|| format!("Failed to connect to LanceDB at {}", path_str))?;

        Ok(Self { db, db_path })
    }

    /// Get or create the chunks table
    async fn get_or_create_table(&self) -> Result<Table> {
        let table_names = self.db.table_names().execute().await?;

        if table_names.contains(&TABLE_NAME.to_string()) {
            debug!("Opening existing table: {}", TABLE_NAME);
            self.db
                .open_table(TABLE_NAME)
                .execute()
                .await
                .with_context(|| format!("Failed to open table {}", TABLE_NAME))
        } else {
            debug!("Creating new table: {}", TABLE_NAME);
            self.create_table().await
        }
    }

    /// Get the row count for a table, with fallback to MAX_QUERY_ROWS on error.
    /// Logs a warning if count_rows fails.
    async fn get_row_count_or_max(table: &Table) -> usize {
        match table.count_rows(None).await {
            Ok(count) => count,
            Err(e) => {
                warn!(
                    error = %e,
                    fallback = MAX_QUERY_ROWS,
                    "Failed to count rows, using fallback limit"
                );
                MAX_QUERY_ROWS
            }
        }
    }

    /// Create the chunks table with the correct schema
    async fn create_table(&self) -> Result<Table> {
        let schema = Self::table_schema();

        // Create empty batches with schema to initialize table
        let batches = RecordBatchIterator::new(vec![], Arc::new(schema));

        self.db
            .create_table(TABLE_NAME, Box::new(batches))
            .execute()
            .await
            .with_context(|| "Failed to create chunks table")
    }

    /// Define the Arrow schema for the chunks table
    fn table_schema() -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("file_path", DataType::Utf8, false),
            Field::new("start_line", DataType::Int32, false),
            Field::new("end_line", DataType::Int32, false),
            Field::new("language", DataType::Utf8, true),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    VECTOR_DIMENSION,
                ),
                false,
            ),
            Field::new("mtime", DataType::Int64, false),
            Field::new("file_header", DataType::Utf8, true),
            // Symbol metadata fields
            Field::new("semantic_kind", DataType::Utf8, true),
            Field::new("symbol_name", DataType::Utf8, true),
            Field::new("signature", DataType::Utf8, true),
            Field::new("parent", DataType::Utf8, true),
            Field::new("visibility", DataType::Utf8, true),
        ])
    }

    /// Insert chunks into the database
    pub async fn insert_chunks(&self, chunks: Vec<IndexedChunk>) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let table = self.get_or_create_table().await?;

        let batch = Self::chunks_to_record_batch(&chunks)?;
        let batches = RecordBatchIterator::new(vec![Ok(batch)], Arc::new(Self::table_schema()));

        table
            .add(Box::new(batches))
            .execute()
            .await
            .with_context(|| "Failed to insert chunks")?;

        info!("Inserted {} chunks into database", chunks.len());

        Ok(())
    }

    /// Convert IndexedChunks to Arrow RecordBatch
    fn chunks_to_record_batch(chunks: &[IndexedChunk]) -> Result<RecordBatch> {
        let ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
        let contents: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        let file_paths: Vec<&str> = chunks.iter().map(|c| c.file_path.as_str()).collect();
        let start_lines: Vec<i32> = chunks.iter().map(|c| c.start_line as i32).collect();
        let end_lines: Vec<i32> = chunks.iter().map(|c| c.end_line as i32).collect();
        let languages: Vec<Option<&str>> = chunks
            .iter()
            .map(|c| c.language.as_deref())
            .collect();
        let mtimes: Vec<i64> = chunks.iter().map(|c| c.mtime).collect();
        let file_headers: Vec<Option<&str>> = chunks
            .iter()
            .map(|c| c.file_header.as_deref())
            .collect();

        // Symbol metadata fields
        let semantic_kinds: Vec<Option<&str>> = chunks
            .iter()
            .map(|c| c.semantic_kind.as_deref())
            .collect();
        let symbol_names: Vec<Option<&str>> = chunks
            .iter()
            .map(|c| c.symbol_name.as_deref())
            .collect();
        let signatures: Vec<Option<&str>> = chunks
            .iter()
            .map(|c| c.signature.as_deref())
            .collect();
        let parents: Vec<Option<&str>> = chunks
            .iter()
            .map(|c| c.parent.as_deref())
            .collect();
        let visibilities: Vec<Option<&str>> = chunks
            .iter()
            .map(|c| c.visibility.as_deref())
            .collect();

        // Build vector array
        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            chunks
                .iter()
                .map(|c| Some(c.vector.iter().map(|&v| Some(v)))),
            VECTOR_DIMENSION,
        );

        let schema = Arc::new(Self::table_schema());

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(ids)),
                Arc::new(StringArray::from(contents)),
                Arc::new(StringArray::from(file_paths)),
                Arc::new(Int32Array::from(start_lines)),
                Arc::new(Int32Array::from(end_lines)),
                Arc::new(StringArray::from(languages)),
                Arc::new(vector_array),
                Arc::new(Int64Array::from(mtimes)),
                Arc::new(StringArray::from(file_headers)),
                Arc::new(StringArray::from(semantic_kinds)),
                Arc::new(StringArray::from(symbol_names)),
                Arc::new(StringArray::from(signatures)),
                Arc::new(StringArray::from(parents)),
                Arc::new(StringArray::from(visibilities)),
            ],
        )
        .with_context(|| "Failed to create RecordBatch")
    }

    /// Perform vector similarity search
    pub async fn search(&self, vector: Vec<f32>, limit: usize) -> Result<Vec<SearchResult>> {
        let table = self.get_or_create_table().await?;

        let results = table
            .vector_search(vector)
            .with_context(|| "Failed to create vector search query")?
            .limit(limit)
            .execute()
            .await
            .with_context(|| "Failed to execute vector search")?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .with_context(|| "Failed to collect search results")?;

        let mut search_results = Vec::new();

        for batch in batches {
            let contents = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow::anyhow!("Missing content column"))?;

            let file_paths = batch
                .column_by_name("file_path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow::anyhow!("Missing file_path column"))?;

            let start_lines = batch
                .column_by_name("start_line")
                .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
                .ok_or_else(|| anyhow::anyhow!("Missing start_line column"))?;

            let end_lines = batch
                .column_by_name("end_line")
                .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
                .ok_or_else(|| anyhow::anyhow!("Missing end_line column"))?;

            let file_headers = batch
                .column_by_name("file_header")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            // LanceDB returns _distance column for similarity score
            let distances = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::Float32Array>());

            for i in 0..batch.num_rows() {
                let score = distances
                    .map(|d| 1.0 / (1.0 + d.value(i))) // Convert distance to similarity
                    .unwrap_or(1.0);

                let file_header = file_headers
                    .and_then(|h| {
                        if h.is_null(i) {
                            None
                        } else {
                            Some(h.value(i).to_string())
                        }
                    });

                search_results.push(SearchResult {
                    content: contents.value(i).to_string(),
                    file_path: file_paths.value(i).to_string(),
                    start_line: start_lines.value(i) as usize,
                    end_line: end_lines.value(i) as usize,
                    score,
                    file_header,
                });
            }
        }

        Ok(search_results)
    }

    /// Get modification times for all indexed files
    pub async fn get_file_mtimes(&self) -> Result<HashMap<PathBuf, i64>> {
        let table = self.get_or_create_table().await?;

        // Get total row count to ensure we query all rows
        let total_rows = Self::get_row_count_or_max(&table).await;

        let results = table
            .query()
            .select(lancedb::query::Select::Columns(vec![
                "file_path".to_string(),
                "mtime".to_string(),
            ]))
            .limit(total_rows) // Explicitly request all rows
            .execute()
            .await
            .with_context(|| "Failed to query file mtimes")?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .with_context(|| "Failed to collect mtime results")?;

        let mut mtimes = HashMap::new();

        for batch in batches {
            let file_paths = batch
                .column_by_name("file_path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow::anyhow!("Missing file_path column"))?;

            let mtime_col = batch
                .column_by_name("mtime")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
                .ok_or_else(|| anyhow::anyhow!("Missing mtime column"))?;

            for i in 0..batch.num_rows() {
                let path = PathBuf::from(file_paths.value(i));
                let mtime = mtime_col.value(i);
                // Keep the most recent mtime for each file
                mtimes
                    .entry(path)
                    .and_modify(|e| {
                        if mtime > *e {
                            *e = mtime
                        }
                    })
                    .or_insert(mtime);
            }
        }

        Ok(mtimes)
    }

    /// Delete all chunks for a given file path
    pub async fn delete_by_file(&self, path: &Path) -> Result<()> {
        let table = self.get_or_create_table().await?;
        let path_str = path.to_string_lossy();

        table
            .delete(&format!("file_path = '{}'", path_str))
            .await
            .with_context(|| format!("Failed to delete chunks for file: {}", path_str))?;

        debug!("Deleted chunks for file: {}", path_str);

        Ok(())
    }

    /// List all unique file paths in the index, optionally filtered by pattern
    pub async fn list_files(&self, pattern: Option<&str>) -> Result<Vec<String>> {
        let table = self.get_or_create_table().await?;

        // Get total row count to ensure we query all rows
        let total_rows = Self::get_row_count_or_max(&table).await;

        let results = table
            .query()
            .select(lancedb::query::Select::Columns(vec!["file_path".to_string()]))
            .limit(total_rows) // Explicitly request all rows
            .execute()
            .await
            .with_context(|| "Failed to query file paths")?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .with_context(|| "Failed to collect file paths")?;

        let mut files: std::collections::HashSet<String> = std::collections::HashSet::new();

        for batch in batches {
            let file_paths = batch
                .column_by_name("file_path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow::anyhow!("Missing file_path column"))?;

            for i in 0..batch.num_rows() {
                files.insert(file_paths.value(i).to_string());
            }
        }

        // Filter by pattern if provided
        let mut result: Vec<String> = if let Some(pat) = pattern {
            let glob_pattern =
                glob::Pattern::new(pat).with_context(|| format!("Invalid glob pattern: {}", pat))?;

            files
                .into_iter()
                .filter(|f| glob_pattern.matches(f))
                .collect()
        } else {
            files.into_iter().collect()
        };

        result.sort();
        Ok(result)
    }

    /// Get total count of chunks in the database
    pub async fn count_chunks(&self) -> Result<usize> {
        let table = self.get_or_create_table().await?;

        let count = table
            .count_rows(None)
            .await
            .with_context(|| "Failed to count chunks")?;

        Ok(count)
    }

    /// Clear all data from the database
    pub async fn clear(&self) -> Result<()> {
        let table_names = self.db.table_names().execute().await?;

        if table_names.contains(&TABLE_NAME.to_string()) {
            self.db
                .drop_table(TABLE_NAME)
                .await
                .with_context(|| "Failed to drop chunks table")?;
        }

        info!("Cleared all data from database");
        Ok(())
    }

    /// Get the database path
    pub fn path(&self) -> &Path {
        &self.db_path
    }

    /// Get all chunks from the database.
    ///
    /// This method is used for building secondary indices like BM25.
    /// Returns all indexed chunks with their metadata (excluding vectors for efficiency).
    pub async fn get_all_chunks(&self) -> Result<Vec<IndexedChunk>> {
        let table = self.get_or_create_table().await?;

        // Get total row count to ensure we query all rows
        let total_rows = Self::get_row_count_or_max(&table).await;

        let results = table
            .query()
            .select(lancedb::query::Select::Columns(vec![
                "id".to_string(),
                "content".to_string(),
                "file_path".to_string(),
                "start_line".to_string(),
                "end_line".to_string(),
                "language".to_string(),
                "mtime".to_string(),
                "file_header".to_string(),
                "semantic_kind".to_string(),
                "symbol_name".to_string(),
                "signature".to_string(),
                "parent".to_string(),
                "visibility".to_string(),
            ]))
            .limit(total_rows) // Explicitly request all rows
            .execute()
            .await
            .with_context(|| "Failed to query all chunks")?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .with_context(|| "Failed to collect chunks")?;

        let mut chunks = Vec::new();

        for batch in batches {
            let ids = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow::anyhow!("Missing id column"))?;

            let contents = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow::anyhow!("Missing content column"))?;

            let file_paths = batch
                .column_by_name("file_path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or_else(|| anyhow::anyhow!("Missing file_path column"))?;

            let start_lines = batch
                .column_by_name("start_line")
                .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
                .ok_or_else(|| anyhow::anyhow!("Missing start_line column"))?;

            let end_lines = batch
                .column_by_name("end_line")
                .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
                .ok_or_else(|| anyhow::anyhow!("Missing end_line column"))?;

            let languages = batch
                .column_by_name("language")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let mtimes = batch
                .column_by_name("mtime")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
                .ok_or_else(|| anyhow::anyhow!("Missing mtime column"))?;

            let file_headers = batch
                .column_by_name("file_header")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let semantic_kinds = batch
                .column_by_name("semantic_kind")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let symbol_names = batch
                .column_by_name("symbol_name")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let signatures = batch
                .column_by_name("signature")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let parents = batch
                .column_by_name("parent")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let visibilities = batch
                .column_by_name("visibility")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            for i in 0..batch.num_rows() {
                let language = languages
                    .and_then(|l| {
                        if l.is_null(i) {
                            None
                        } else {
                            Some(l.value(i).to_string())
                        }
                    });

                let file_header = file_headers
                    .and_then(|h| {
                        if h.is_null(i) {
                            None
                        } else {
                            Some(h.value(i).to_string())
                        }
                    });

                let semantic_kind = semantic_kinds
                    .and_then(|k| {
                        if k.is_null(i) {
                            None
                        } else {
                            Some(k.value(i).to_string())
                        }
                    });

                let symbol_name = symbol_names
                    .and_then(|n| {
                        if n.is_null(i) {
                            None
                        } else {
                            Some(n.value(i).to_string())
                        }
                    });

                let signature = signatures
                    .and_then(|s| {
                        if s.is_null(i) {
                            None
                        } else {
                            Some(s.value(i).to_string())
                        }
                    });

                let parent = parents
                    .and_then(|p| {
                        if p.is_null(i) {
                            None
                        } else {
                            Some(p.value(i).to_string())
                        }
                    });

                let visibility = visibilities
                    .and_then(|v| {
                        if v.is_null(i) {
                            None
                        } else {
                            Some(v.value(i).to_string())
                        }
                    });

                chunks.push(IndexedChunk {
                    id: ids.value(i).to_string(),
                    content: contents.value(i).to_string(),
                    file_path: file_paths.value(i).to_string(),
                    start_line: start_lines.value(i) as usize,
                    end_line: end_lines.value(i) as usize,
                    language,
                    vector: Vec::new(), // Empty vector - not needed for BM25
                    mtime: mtimes.value(i),
                    file_header,
                    semantic_kind,
                    symbol_name,
                    signature,
                    parent,
                    visibility,
                });
            }
        }

        debug!("Retrieved {} chunks from database", chunks.len());
        Ok(chunks)
    }
}

// Required for arrow streams
use futures::TryStreamExt;
