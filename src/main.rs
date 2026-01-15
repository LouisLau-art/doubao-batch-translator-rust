//! Main entry point for Doubao Batch Translator CLI

#![forbid(unsafe_code)]

use clap::Parser;
use dotenvy::dotenv;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod cli;
mod core;
mod processors;
mod server;
mod utils;

use cli::commands::Commands;

/// Doubao Batch Translator - High-performance Rust translation tool
#[derive(Parser, Debug)]
#[command(name = "doubao-translator", version, about, long_about = None)]
struct Args {
    /// API key for Doubao (optional, defaults to ARK_API_KEY env var)
    #[arg(long)]
    api_key: Option<String>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Maximum concurrent requests
    #[arg(long)]
    max_concurrent: Option<usize>,

    /// Maximum requests per second
    #[arg(long)]
    max_rps: Option<f64>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenv().ok();

    // Initialize logging
    let log_level = if std::env::var("RUST_LOG").is_ok() {
        std::env::var("RUST_LOG").unwrap()
    } else {
        "info".to_string()
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}={}", env!("CARGO_PKG_NAME"), log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Override config with CLI args if provided
    if let Some(api_key) = args.api_key {
        std::env::set_var("ARK_API_KEY", api_key);
    }

    if args.verbose {
        std::env::set_var("RUST_LOG", "debug");
    }

    // Execute command
    match args.command {
        Some(Commands::Md {
            file,
            output,
            source_lang,
            target_lang,
            recursive,
        }) => {
            cli::commands::handle_md(file, output, source_lang, target_lang, recursive).await?;
        }
        Some(Commands::Epub {
            file,
            output,
            source_lang,
            target_lang,
            auto_approve,
        }) => {
            cli::commands::handle_epub(file, output, source_lang, target_lang, auto_approve).await?;
        }
        Some(Commands::Server {
            host,
            port,
            debug,
        }) => {
            cli::commands::handle_server(host, port, debug).await?;
        }
        Some(Commands::CheckUntranslated { dir }) => {
            cli::commands::handle_check_untranslated(dir).await?;
        }
        Some(Commands::ApplyFix { json }) => {
            cli::commands::handle_apply_fix(json).await?;
        }
        None => {
            println!("Please specify a command. Use --help for more information.");
        }
    }

    Ok(())
}