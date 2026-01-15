//! Doubao Batch Translator - High-performance Rust translation library
//!
//! This library provides asynchronous translation capabilities with support for
//! Markdown, ePub files, and HTTP API services.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]

pub mod core;
pub mod processors;
pub mod server;
pub mod cli;
pub mod utils;

// Re-export key types for convenience
pub use core::{
    client::AsyncTranslator,
    config::TranslatorConfig,
    models::{Model, LaneType, TranslationRequest, TranslationResult, TokenUsage},
    errors::TranslationError,
};

pub use processors::{
    markdown::MarkdownProcessor,
    epub::EpubProcessor,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");