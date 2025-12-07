use coderag::storage::IndexedChunk;

/// Create a test chunk with default values
pub fn create_test_chunk(id: &str, content: &str, file_path: &str) -> IndexedChunk {
    IndexedChunk {
        id: id.to_string(),
        content: content.to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 10,
        language: Some("rust".to_string()),
        vector: vec![0.0; 768],
        mtime: 0,
        file_header: None,
        semantic_kind: None,
        symbol_name: None,
        signature: None,
        parent: None,
        visibility: None,
    }
}

/// Create a test chunk with custom mtime
pub fn create_test_chunk_with_mtime(
    id: &str,
    content: &str,
    file_path: &str,
    _mtime: u64,
) -> IndexedChunk {
    let chunk = create_test_chunk(id, content, file_path);
    // Note: mtime is in the chunk structure now
    chunk
}

/// Create a test chunk with custom vector
pub fn create_test_chunk_with_vector(
    id: &str,
    content: &str,
    file_path: &str,
    vector: Vec<f32>,
) -> IndexedChunk {
    IndexedChunk {
        id: id.to_string(),
        content: content.to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 10,
        language: Some("rust".to_string()),
        vector,
        mtime: 0,
        file_header: None,
        semantic_kind: None,
        symbol_name: None,
        signature: None,
        parent: None,
        visibility: None,
    }
}