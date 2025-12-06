//! Parallel indexing implementation

pub mod parallel;
pub mod errors;
pub mod pipeline;

pub use parallel::ParallelIndexer;
pub use errors::{FileError, ProcessingStage, ErrorCollector, ErrorReport};
pub use pipeline::{FileContent, RawChunk, ProcessingResult};