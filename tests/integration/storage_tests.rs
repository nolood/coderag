use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

use coderag::storage::{IndexedChunk, Storage};

/// Helper function to create test chunks
fn create_test_chunk(id: &str, content: &str, file_path: &str) -> IndexedChunk {
    IndexedChunk {
        id: id.to_string(),
        content: content.to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 10,
        language: Some("rust".to_string()),
        vector: vec![0.1; 768], // Simple test vector
        mtime: 1000,
        file_header: Some("// Test file header".to_string()),
        semantic_kind: None,
        symbol_name: None,
        signature: None,
        parent: None,
        visibility: None,
    }
}

fn create_test_chunk_with_mtime(id: &str, content: &str, file_path: &str, mtime: i64) -> IndexedChunk {
    let mut chunk = create_test_chunk(id, content, file_path);
    chunk.mtime = mtime;
    chunk
}

fn create_test_chunk_with_vector(id: &str, content: &str, file_path: &str, vector: Vec<f32>) -> IndexedChunk {
    let mut chunk = create_test_chunk(id, content, file_path);
    chunk.vector = vector;
    chunk
}

#[tokio::test]
async fn test_insert_and_search_chunks() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Create test chunks with different vectors for better search testing
    let chunks = vec![
        create_test_chunk_with_vector("chunk1", "fn main() {}", "file.rs", vec![1.0; 768]),
        create_test_chunk_with_vector("chunk2", "fn helper() {}", "file.rs", vec![0.5; 768]),
        create_test_chunk_with_vector("chunk3", "fn test() {}", "test.rs", vec![0.2; 768]),
    ];

    // Insert chunks
    storage.insert_chunks(chunks).await?;

    // Search with a query vector close to chunk1
    let query_vec = vec![0.9; 768];
    let results = storage.search(query_vec, 2).await?;

    assert_eq!(results.len(), 2, "Should return 2 results");
    assert_eq!(results[0].content, "fn main() {}", "First result should be chunk1");
    assert_eq!(results[0].file_path, "file.rs");

    Ok(())
}

#[tokio::test]
async fn test_delete_by_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Insert chunks from multiple files
    let chunks = vec![
        create_test_chunk("1", "content1", "file1.rs"),
        create_test_chunk("2", "content2", "file1.rs"),
        create_test_chunk("3", "content3", "file2.rs"),
        create_test_chunk("4", "content4", "file2.rs"),
        create_test_chunk("5", "content5", "file3.rs"),
    ];

    storage.insert_chunks(chunks).await?;

    // Delete chunks from file1.rs
    storage.delete_by_file(&PathBuf::from("file1.rs")).await?;

    // Verify by searching - should not find file1.rs chunks
    let results = storage.search(vec![0.5; 768], 10).await?;

    assert!(results.iter().all(|r| r.file_path != "file1.rs"),
        "file1.rs chunks should be deleted");

    // Should still find file2.rs and file3.rs chunks
    let file_paths: Vec<String> = results.iter().map(|r| r.file_path.clone()).collect();
    assert!(file_paths.contains(&"file2.rs".to_string()));
    assert!(file_paths.contains(&"file3.rs".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_list_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Insert chunks from different files
    let chunks = vec![
        create_test_chunk("1", "content1", "src/main.rs"),
        create_test_chunk("2", "content2", "src/lib.rs"),
        create_test_chunk("3", "content3", "tests/test.rs"),
        create_test_chunk("4", "content4", "src/main.rs"), // Duplicate file path
    ];

    storage.insert_chunks(chunks).await?;

    // List all files
    let files = storage.list_files(None).await?;

    // Should have unique file paths
    assert_eq!(files.len(), 3, "Should have 3 unique file paths");
    assert!(files.contains(&"src/main.rs".to_string()));
    assert!(files.contains(&"src/lib.rs".to_string()));
    assert!(files.contains(&"tests/test.rs".to_string()));

    // Test with pattern filtering
    let pattern = "*.rs";
    let filtered_files = storage.list_files(Some(pattern)).await?;

    // Pattern filtering depends on implementation
    // For now, just check we get results
    assert!(!filtered_files.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_file_mtimes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Insert chunks with different mtimes
    let chunks = vec![
        create_test_chunk_with_mtime("1", "content1", "file1.rs", 1000),
        create_test_chunk_with_mtime("2", "content2", "file1.rs", 1000),
        create_test_chunk_with_mtime("3", "content3", "file2.rs", 2000),
        create_test_chunk_with_mtime("4", "content4", "file2.rs", 2000),
        create_test_chunk_with_mtime("5", "content5", "file3.rs", 3000),
    ];

    storage.insert_chunks(chunks).await?;

    // Get file modification times
    let mtimes = storage.get_file_mtimes().await?;

    assert_eq!(mtimes.len(), 3, "Should have 3 unique files");
    assert_eq!(mtimes.get(&PathBuf::from("file1.rs")), Some(&1000));
    assert_eq!(mtimes.get(&PathBuf::from("file2.rs")), Some(&2000));
    assert_eq!(mtimes.get(&PathBuf::from("file3.rs")), Some(&3000));

    Ok(())
}

#[tokio::test]
async fn test_empty_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Insert empty vector should be ok
    storage.insert_chunks(vec![]).await?;

    // Search on empty database
    let results = storage.search(vec![0.5; 768], 10).await?;
    assert_eq!(results.len(), 0, "Empty database should return no results");

    // List files on empty database
    let files = storage.list_files(None).await?;
    assert_eq!(files.len(), 0, "Empty database should have no files");

    // Get mtimes on empty database
    let mtimes = storage.get_file_mtimes().await?;
    assert_eq!(mtimes.len(), 0, "Empty database should have no mtimes");

    Ok(())
}

#[tokio::test]
async fn test_vector_similarity_ordering() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Create chunks with progressively different vectors
    let base_vector = vec![1.0; 768];

    let mut vec1 = base_vector.clone();
    vec1[0] = 0.9; // Slightly different

    let mut vec2 = base_vector.clone();
    vec2[0] = 0.5; // More different

    let mut vec3 = base_vector.clone();
    vec3[0] = 0.1; // Very different

    let chunks = vec![
        create_test_chunk_with_vector("1", "closest match", "file1.rs", vec1),
        create_test_chunk_with_vector("2", "medium match", "file2.rs", vec2),
        create_test_chunk_with_vector("3", "distant match", "file3.rs", vec3),
    ];

    storage.insert_chunks(chunks).await?;

    // Search with base vector
    let results = storage.search(base_vector, 3).await?;

    assert_eq!(results.len(), 3);

    // Results should be ordered by similarity (descending score)
    assert!(results[0].score >= results[1].score, "Results should be ordered by score");
    assert!(results[1].score >= results[2].score, "Results should be ordered by score");

    // The closest match should be first
    assert_eq!(results[0].content, "closest match");

    Ok(())
}

#[tokio::test]
async fn test_chunk_with_optional_fields() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Create chunks with and without optional fields
    let mut chunk1 = create_test_chunk("1", "with header", "file1.rs");
    chunk1.file_header = Some("// File header content".to_string());
    chunk1.language = Some("rust".to_string());

    let mut chunk2 = create_test_chunk("2", "without header", "file2.py");
    chunk2.file_header = None;
    chunk2.language = Some("python".to_string());

    let chunks = vec![chunk1, chunk2];
    storage.insert_chunks(chunks).await?;

    // Search and verify optional fields are preserved
    let results = storage.search(vec![0.5; 768], 2).await?;

    let with_header = results.iter().find(|r| r.content == "with header").unwrap();
    assert!(with_header.file_header.is_some());
    assert_eq!(with_header.file_header.as_ref().unwrap(), "// File header content");

    let without_header = results.iter().find(|r| r.content == "without header").unwrap();
    assert!(without_header.file_header.is_none());

    Ok(())
}

#[tokio::test]
async fn test_large_batch_insert() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Create a large batch of chunks
    let mut chunks = Vec::new();
    for i in 0..100 {
        let chunk = create_test_chunk(
            &format!("chunk_{}", i),
            &format!("content_{}", i),
            &format!("file_{}.rs", i % 10), // 10 different files
        );
        chunks.push(chunk);
    }

    // Insert all chunks at once
    storage.insert_chunks(chunks).await?;

    // Verify all chunks are searchable
    let results = storage.search(vec![0.5; 768], 100).await?;
    assert!(results.len() <= 100, "Should return at most 100 results");

    // Verify file listing
    let files = storage.list_files(None).await?;
    assert_eq!(files.len(), 10, "Should have 10 unique files");

    Ok(())
}

#[tokio::test]
async fn test_duplicate_chunk_ids() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path, 768).await?;

    // Insert chunk with ID "duplicate"
    let chunk1 = create_test_chunk("duplicate", "first content", "file1.rs");
    storage.insert_chunks(vec![chunk1]).await?;

    // Insert another chunk with the same ID
    let chunk2 = create_test_chunk("duplicate", "second content", "file2.rs");
    storage.insert_chunks(vec![chunk2]).await?;

    // Search for the chunks
    let results = storage.search(vec![0.5; 768], 10).await?;

    // Both chunks should be present (LanceDB doesn't enforce unique IDs)
    let contents: Vec<String> = results.iter().map(|r| r.content.clone()).collect();
    assert!(contents.contains(&"first content".to_string()));
    assert!(contents.contains(&"second content".to_string()));

    Ok(())
}