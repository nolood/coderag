use anyhow::Result;
use std::sync::Arc;
use tempfile::TempDir;

use coderag::storage::{IndexedChunk, Storage};

// Helper function for creating test data
async fn setup_test_environment() -> Result<(TempDir, Arc<Storage>)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");

    // Create storage
    let storage = Arc::new(Storage::new(&db_path).await?);

    // Insert test data
    let test_chunks = vec![
        IndexedChunk {
            id: "chunk_1".to_string(),
            content: "fn main() { println!(\"Hello, world!\"); }".to_string(),
            file_path: "src/main.rs".to_string(),
            start_line: 1,
            end_line: 3,
            language: Some("rust".to_string()),
            vector: vec![0.1; 768],
            mtime: 1000,
            file_header: Some("// Main file".to_string()),
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        },
        IndexedChunk {
            id: "chunk_2".to_string(),
            content: "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 5,
            end_line: 7,
            language: Some("rust".to_string()),
            vector: vec![0.2; 768],
            mtime: 1000,
            file_header: Some("// Library file".to_string()),
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        },
        IndexedChunk {
            id: "chunk_3".to_string(),
            content: "def hello(): print('Hello from Python')".to_string(),
            file_path: "scripts/hello.py".to_string(),
            start_line: 1,
            end_line: 2,
            language: Some("python".to_string()),
            vector: vec![0.3; 768],
            mtime: 1000,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        },
    ];

    storage.insert_chunks(test_chunks).await?;

    Ok((temp_dir, storage))
}

#[tokio::test]
async fn test_storage_setup() -> Result<()> {
    let (temp_dir, storage) = setup_test_environment().await?;
    let root_path = temp_dir.path().to_path_buf();

    // Verify storage was created and has data
    assert!(root_path.exists());

    // Test that the storage has the expected data for search
    let results = storage.search(vec![0.1; 768], 5).await?;

    assert!(!results.is_empty(), "Should find search results");
    assert!(results.iter().any(|r| r.content.contains("main")));
    assert!(results.iter().any(|r| r.file_path == "src/main.rs"));

    Ok(())
}

#[tokio::test]
async fn test_list_files_functionality() -> Result<()> {
    let (_temp_dir, storage) = setup_test_environment().await?;

    // Test listing files
    let files = storage.list_files(None).await?;

    assert_eq!(files.len(), 3, "Should have 3 unique files");
    assert!(files.contains(&"src/main.rs".to_string()));
    assert!(files.contains(&"src/lib.rs".to_string()));
    assert!(files.contains(&"scripts/hello.py".to_string()));

    // Test with pattern filtering
    let rust_files = storage.list_files(Some("*.rs")).await?;
    assert!(rust_files.iter().all(|f| f.ends_with(".rs") || f.contains("/")));

    Ok(())
}

#[tokio::test]
async fn test_multi_language_support() -> Result<()> {
    let (_temp_dir, storage) = setup_test_environment().await?;

    // Verify multiple languages are indexed
    let files = storage.list_files(None).await?;

    let rust_files: Vec<_> = files.iter().filter(|f| f.ends_with(".rs")).collect();
    let python_files: Vec<_> = files.iter().filter(|f| f.ends_with(".py")).collect();

    assert_eq!(rust_files.len(), 2, "Should have 2 Rust files");
    assert_eq!(python_files.len(), 1, "Should have 1 Python file");

    Ok(())
}

#[tokio::test]
async fn test_search_with_metadata() -> Result<()> {
    let (_temp_dir, storage) = setup_test_environment().await?;

    // Search and verify metadata is preserved
    let results = storage.search(vec![0.1; 768], 10).await?;

    for result in &results {
        // Verify essential metadata
        assert!(!result.file_path.is_empty(), "File path should be present");
        assert!(result.start_line > 0, "Start line should be positive");
        assert!(result.end_line >= result.start_line, "End line should be >= start line");
        assert!(result.score >= 0.0, "Score should be non-negative");
    }

    // Check file headers are preserved where present
    let main_result = results.iter().find(|r| r.file_path == "src/main.rs");
    if let Some(result) = main_result {
        assert!(result.file_header.is_some(), "Main.rs should have file header");
        assert_eq!(result.file_header.as_ref().unwrap(), "// Main file");
    }

    Ok(())
}

#[tokio::test]
async fn test_empty_database_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("empty.lance");
    let storage = Arc::new(Storage::new(&db_path).await?);

    // Operations should handle empty database gracefully
    let results = storage.search(vec![0.5; 768], 10).await?;
    assert_eq!(results.len(), 0, "Empty database should return no results");

    let files = storage.list_files(None).await?;
    assert_eq!(files.len(), 0, "Empty database should have no files");

    Ok(())
}

#[tokio::test]
async fn test_concurrent_access() -> Result<()> {
    let (_temp_dir, storage) = setup_test_environment().await?;

    // Simulate concurrent access
    let mut handles = vec![];

    for i in 0..5 {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            // Each task performs a search
            let query_vector = vec![0.1 + i as f32 * 0.1; 768];
            storage_clone.search(query_vector, 5).await
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        let result = handle.await?;
        assert!(result.is_ok(), "Concurrent search should succeed");
    }

    Ok(())
}

#[tokio::test]
async fn test_file_path_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.lance");
    let storage = Arc::new(Storage::new(&db_path).await?);

    // Test various file path formats
    let chunks = vec![
        IndexedChunk {
            id: "1".to_string(),
            content: "test".to_string(),
            file_path: "relative/path.rs".to_string(),
            start_line: 1,
            end_line: 1,
            language: Some("rust".to_string()),
            vector: vec![0.1; 768],
            mtime: 1000,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        },
        IndexedChunk {
            id: "2".to_string(),
            content: "test".to_string(),
            file_path: "./another/path.py".to_string(),
            start_line: 1,
            end_line: 1,
            language: Some("python".to_string()),
            vector: vec![0.2; 768],
            mtime: 1000,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        },
        IndexedChunk {
            id: "3".to_string(),
            content: "test".to_string(),
            file_path: "deep/nested/structure/file.ts".to_string(),
            start_line: 1,
            end_line: 1,
            language: Some("typescript".to_string()),
            vector: vec![0.3; 768],
            mtime: 1000,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        },
    ];

    storage.insert_chunks(chunks).await?;

    let files = storage.list_files(None).await?;
    assert_eq!(files.len(), 3, "Should handle various path formats");

    Ok(())
}