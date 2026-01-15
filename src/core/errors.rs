//! Custom error types for translation operations

use thiserror::Error;

/// Translation-related errors
#[derive(Error, Debug)]
pub enum TranslationError {
    /// API request failed
    #[error("API error: {status} - {message}")]
    ApiError {
        status: u16,
        message: String,
    },

    /// Rate limit exceeded
    #[error("Rate limit exceeded. Retry after {retry_after:?} seconds")]
    RateLimitError {
        retry_after: Option<u64>,
    },

    /// Token quota exceeded
    #[error("Token quota exceeded for today")]
    QuotaExceededError,

    /// Network error
    #[error("Network error: {message}")]
    NetworkError {
        message: String,
    },

    /// Invalid response from API
    #[error("Invalid response: {message}")]
    InvalidResponseError {
        message: String,
    },

    /// Request timeout
    #[error("Request timeout")]
    TimeoutError,

    /// File operation error
    #[error("File error: {path} - {message}")]
    FileError {
        path: String,
        message: String,
    },

    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigError {
        message: String,
    },

    /// Invalid file format
    #[error("Invalid file format: {format}")]
    InvalidFormat {
        format: String,
    },

    /// Missing required field
    #[error("Missing required field: {field}")]
    MissingField {
        field: String,
    },

    /// Wrapper for anyhow errors
    #[error("Internal error: {0}")]
    InternalError(String),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Reqwest error
    #[error("HTTP client error: {0}")]
    HttpError(#[from] reqwest::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// YAML error
    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),
}

impl From<anyhow::Error> for TranslationError {
    fn from(err: anyhow::Error) -> Self {
        TranslationError::InternalError(err.to_string())
    }
}

/// Result type for translation operations
pub type Result<T> = std::result::Result<T, TranslationError>;