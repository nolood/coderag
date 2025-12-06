// Main integration test file that includes all test modules

mod integration {
    pub mod storage_tests;
    pub mod workflow_tests;
    pub mod language_tests;
    pub mod mcp_server_tests;
}

mod helpers {
    pub mod test_harness;
    pub mod mock_embeddings;
    pub mod test_utils;
}