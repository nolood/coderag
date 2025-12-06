//! Error handling patterns and implementations

use thiserror::Error;
use std::fmt;
use std::io;

/// Application error types
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Validation error: {field} - {message}")]
    Validation { field: String, message: String },

    #[error("Not found: {resource}")]
    NotFound { resource: String },

    #[error("Unauthorized access")]
    Unauthorized,

    #[error("Internal server error: {0}")]
    Internal(String),
}

/// Custom Result type
pub type Result<T> = std::result::Result<T, AppError>;

/// Error handler for converting errors to HTTP responses
pub fn handle_error(error: AppError) -> (u16, String) {
    match error {
        AppError::NotFound { resource } => (404, format!("Resource not found: {}", resource)),
        AppError::Validation { field, message } => (400, format!("Validation error on {}: {}", field, message)),
        AppError::Unauthorized => (401, "Unauthorized access".to_string()),
        AppError::Database(msg) => (500, format!("Database error: {}", msg)),
        AppError::Io(err) => (500, format!("IO error: {}", err)),
        AppError::Internal(msg) => (500, format!("Internal error: {}", msg)),
    }
}

/// Error context trait for adding context to errors
pub trait ErrorContext<T> {
    fn context(self, msg: impl Into<String>) -> Result<T>;
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T> ErrorContext<T> for Result<T> {
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.map_err(|_| AppError::Internal(msg.into()))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|_| AppError::Internal(f()))
    }
}

/// Example of error propagation
pub fn process_data(input: &str) -> Result<String> {
    validate_input(input)?;
    let parsed = parse_input(input)?;
    transform_data(parsed)
}

fn validate_input(input: &str) -> Result<()> {
    if input.is_empty() {
        return Err(AppError::Validation {
            field: "input".to_string(),
            message: "Cannot be empty".to_string(),
        });
    }
    Ok(())
}

fn parse_input(input: &str) -> Result<Vec<String>> {
    Ok(input.split(',').map(|s| s.to_string()).collect())
}

fn transform_data(data: Vec<String>) -> Result<String> {
    Ok(data.join(";"))
}