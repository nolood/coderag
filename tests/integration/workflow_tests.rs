use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;

use coderag::{
    indexer::Chunker,
    storage::{IndexedChunk, Storage},
};

// Use a simple mock embedder for tests
fn generate_mock_embedding(text: &str, dimension: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let hash = hasher.finish();

    // Generate deterministic vector from hash
    let mut vector = Vec::with_capacity(dimension);
    let mut seed = hash;

    for _ in 0..dimension {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let value = ((seed / 65536) % 1000) as f32 / 1000.0;
        vector.push(value);
    }

    // Normalize vector
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for v in vector.iter_mut() {
            *v /= magnitude;
        }
    }

    vector
}

#[tokio::test]
async fn test_full_workflow_index_and_search() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let project_path = temp_dir.path().join("test_project");
    std::fs::create_dir_all(&project_path)?;

    // Create sample Rust files
    let main_content = r#"
fn main() {
    println!("Hello, world!");
    let result = binary_search(&[1, 2, 3, 4, 5], 3);
    println!("Found at: {:?}", result);
}

fn binary_search(arr: &[i32], target: i32) -> Option<usize> {
    let mut left = 0;
    let mut right = arr.len();

    while left < right {
        let mid = left + (right - left) / 2;
        match arr[mid].cmp(&target) {
            std::cmp::Ordering::Equal => return Some(mid),
            std::cmp::Ordering::Less => left = mid + 1,
            std::cmp::Ordering::Greater => right = mid,
        }
    }
    None
}
"#;

    let lib_content = r#"
pub mod search {
    pub fn linear_search<T: PartialEq>(arr: &[T], target: &T) -> Option<usize> {
        for (i, item) in arr.iter().enumerate() {
            if item == target {
                return Some(i);
            }
        }
        None
    }
}

pub mod sort {
    pub fn bubble_sort<T: Ord>(arr: &mut [T]) {
        let n = arr.len();
        for i in 0..n {
            for j in 0..n - i - 1 {
                if arr[j] > arr[j + 1] {
                    arr.swap(j, j + 1);
                }
            }
        }
    }
}
"#;

    // Write files
    let main_path = project_path.join("main.rs");
    let lib_path = project_path.join("lib.rs");
    std::fs::write(&main_path, main_content)?;
    std::fs::write(&lib_path, lib_content)?;

    // Step 1: Chunk the files
    let chunker = Chunker::new(512); // 512 token chunks
    let main_chunks = chunker.chunk_file(&main_path, main_content);
    let lib_chunks = chunker.chunk_file(&lib_path, lib_content);

    assert!(!main_chunks.is_empty(), "main.rs should produce chunks");
    assert!(!lib_chunks.is_empty(), "lib.rs should produce chunks");

    // Step 2: Generate embeddings using mock embedder
    let mut indexed_chunks = Vec::new();
    let mut chunk_id = 0;

    for chunk in main_chunks {
        let vector = generate_mock_embedding(&chunk.content, 768);
        indexed_chunks.push(IndexedChunk {
            id: format!("chunk_{}", chunk_id),
            content: chunk.content,
            file_path: "main.rs".to_string(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            language: chunk.language,
            vector,
            mtime: 0,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        });
        chunk_id += 1;
    }

    for chunk in lib_chunks {
        let vector = generate_mock_embedding(&chunk.content, 768);
        indexed_chunks.push(IndexedChunk {
            id: format!("chunk_{}", chunk_id),
            content: chunk.content,
            file_path: "lib.rs".to_string(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            language: chunk.language,
            vector,
            mtime: 0,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        });
        chunk_id += 1;
    }

    // Step 3: Store chunks in database
    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path).await?;
    storage.insert_chunks(indexed_chunks).await?;

    // Step 4: Search for specific code
    let query = "binary search algorithm implementation";
    let query_vector = generate_mock_embedding(query, 768);
    let search_results = storage.search(query_vector, 5).await?;

    assert!(!search_results.is_empty(), "Should find results for binary search");

    // The binary_search function should be in the results
    let binary_search_found = search_results
        .iter()
        .any(|r| r.content.contains("binary_search"));
    assert!(binary_search_found, "Should find binary_search function");

    // Search for linear search
    let query2 = "linear search implementation";
    let query_vector2 = generate_mock_embedding(query2, 768);
    let search_results2 = storage.search(query_vector2, 5).await?;

    let linear_search_found = search_results2
        .iter()
        .any(|r| r.content.contains("linear_search"));
    assert!(linear_search_found, "Should find linear_search function");

    Ok(())
}

#[tokio::test]
async fn test_incremental_indexing() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let project_path = temp_dir.path().join("test_project");
    std::fs::create_dir_all(&project_path)?;

    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path).await?;
    let chunker = Chunker::new(512);

    // Initial file
    let initial_content = r#"
fn initial_function() {
    println!("Initial version");
}
"#;
    let file_path = project_path.join("file.rs");
    std::fs::write(&file_path, initial_content)?;

    // Index initial version
    let chunks = chunker.chunk_file(&file_path, initial_content);
    let mut indexed_chunks = Vec::new();

    for (i, chunk) in chunks.into_iter().enumerate() {
        let vector = generate_mock_embedding(&chunk.content, 768);
        indexed_chunks.push(IndexedChunk {
            id: format!("v1_chunk_{}", i),
            content: chunk.content,
            file_path: "file.rs".to_string(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            language: chunk.language,
            vector,
            mtime: 1000,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        });
    }

    storage.insert_chunks(indexed_chunks).await?;

    // Verify initial version is indexed
    let results = storage.search(generate_mock_embedding("initial", 768), 5).await?;
    assert!(results.iter().any(|r| r.content.contains("Initial version")));

    // Delete old chunks for the file
    storage.delete_by_file(&PathBuf::from("file.rs")).await?;

    // Update file
    let updated_content = r#"
fn updated_function() {
    println!("Updated version");
}

fn new_function() {
    println!("New addition");
}
"#;
    std::fs::write(&file_path, updated_content)?;

    // Re-index with updated content
    let chunks = chunker.chunk_file(&file_path, updated_content);
    let mut indexed_chunks = Vec::new();

    for (i, chunk) in chunks.into_iter().enumerate() {
        let vector = generate_mock_embedding(&chunk.content, 768);
        indexed_chunks.push(IndexedChunk {
            id: format!("v2_chunk_{}", i),
            content: chunk.content,
            file_path: "file.rs".to_string(),
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            language: chunk.language,
            vector,
            mtime: 2000,
            file_header: None,
            semantic_kind: None,
            symbol_name: None,
            signature: None,
            parent: None,
            visibility: None,
        });
    }

    storage.insert_chunks(indexed_chunks).await?;

    // Verify updated version is indexed
    let results = storage.search(generate_mock_embedding("updated", 768), 5).await?;
    assert!(results.iter().any(|r| r.content.contains("Updated version")));

    // Old content should not be found
    let old_results = storage.search(generate_mock_embedding("initial", 768), 5).await?;
    assert!(!old_results.iter().any(|r| r.content.contains("Initial version")),
        "Old content should be removed");

    Ok(())
}

#[tokio::test]
async fn test_multi_language_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let project_path = temp_dir.path().join("polyglot_project");
    std::fs::create_dir_all(&project_path)?;

    // Create files in different languages
    let rust_code = r#"
fn rust_function() {
    println!("Rust code");
}
"#;

    let python_code = r#"
def python_function():
    print("Python code")
    return 42
"#;

    let typescript_code = r#"
function typescriptFunction(): number {
    console.log("TypeScript code");
    return 42;
}
"#;

    let rust_path = project_path.join("code.rs");
    let python_path = project_path.join("code.py");
    let ts_path = project_path.join("code.ts");

    std::fs::write(&rust_path, rust_code)?;
    std::fs::write(&python_path, python_code)?;
    std::fs::write(&ts_path, typescript_code)?;

    let db_path = temp_dir.path().join("test.lance");
    let storage = Storage::new(&db_path).await?;
    let chunker = Chunker::new(512);

    // Chunk and index all files
    let mut all_indexed_chunks = Vec::new();
    let mut chunk_id = 0;

    for (file_path, content, file_name, expected_lang) in &[
        (&rust_path, rust_code, "code.rs", Some("rust")),
        (&python_path, python_code, "code.py", Some("python")),
        (&ts_path, typescript_code, "code.ts", Some("typescript")),
    ] {
        let chunks = chunker.chunk_file(file_path, content);

        for chunk in chunks {
            // Verify language detection
            assert_eq!(chunk.language.as_deref(), *expected_lang,
                "Language should be detected correctly for {}", file_name);

            let vector = generate_mock_embedding(&chunk.content, 768);
            all_indexed_chunks.push(IndexedChunk {
                id: format!("chunk_{}", chunk_id),
                content: chunk.content,
                file_path: file_name.to_string(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                language: chunk.language,
                vector,
                mtime: 0,
                file_header: None,
                semantic_kind: None,
                symbol_name: None,
                signature: None,
                parent: None,
                visibility: None,
            });
            chunk_id += 1;
        }
    }

    storage.insert_chunks(all_indexed_chunks).await?;

    // Verify all languages are indexed
    let files = storage.list_files(None).await?;
    assert_eq!(files.len(), 3, "Should have 3 files indexed");
    assert!(files.contains(&"code.rs".to_string()));
    assert!(files.contains(&"code.py".to_string()));
    assert!(files.contains(&"code.ts".to_string()));

    // Search for language-specific content
    let rust_results = storage.search(
        generate_mock_embedding("rust_function", 768),
        5
    ).await?;
    assert!(rust_results.iter().any(|r| r.file_path == "code.rs"));

    let python_results = storage.search(
        generate_mock_embedding("python_function", 768),
        5
    ).await?;
    assert!(python_results.iter().any(|r| r.file_path == "code.py"));

    let ts_results = storage.search(
        generate_mock_embedding("typescriptFunction", 768),
        5
    ).await?;
    assert!(ts_results.iter().any(|r| r.file_path == "code.ts"));

    Ok(())
}

#[tokio::test]
async fn test_large_file_chunking() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let project_path = temp_dir.path().join("large_project");
    std::fs::create_dir_all(&project_path)?;

    // Create a large file with many functions
    let mut large_content = String::new();
    for i in 0..50 {
        large_content.push_str(&format!(
            r#"
fn function_{}() {{
    // Function body with some content
    let x = {};
    let y = x * 2;
    println!("Result: {{}}", y);
}}
"#,
            i, i
        ));
    }

    let file_path = project_path.join("large.rs");
    std::fs::write(&file_path, &large_content)?;

    let chunker = Chunker::new(200); // Smaller chunks for testing
    let chunks = chunker.chunk_file(&file_path, &large_content);

    // Should produce multiple chunks
    assert!(chunks.len() > 5, "Large file should produce multiple chunks");

    // Test that chunks have proper line numbers
    for i in 1..chunks.len() {
        assert!(chunks[i].start_line >= chunks[i-1].start_line,
            "Chunks should have increasing line numbers");
    }

    Ok(())
}