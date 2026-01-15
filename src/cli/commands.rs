//! CLI command definitions and handlers

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Commands for Doubao Batch Translator
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Translate Markdown files
    Md {
        /// Input file or directory (required)
        #[arg(short, long)]
        file: PathBuf,

        /// Output file or directory
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Source language (auto-detect if not specified)
        #[arg(long)]
        source_lang: Option<String>,

        /// Target language (default: zh)
        #[arg(short, long, default_value = "zh")]
        target_lang: String,

        /// Recursively translate subdirectories
        #[arg(short, long)]
        recursive: bool,
    },

    /// Translate ePub files
    Epub {
        /// Input file or directory (required)
        #[arg(short, long)]
        file: PathBuf,

        /// Output file or directory (required)
        #[arg(short, long)]
        output: PathBuf,

        /// Source language (auto-detect if not specified)
        #[arg(long)]
        source_lang: Option<String>,

        /// Target language (default: zh)
        #[arg(short, long, default_value = "zh")]
        target_lang: String,

        /// Auto-approve translations
        #[arg(long)]
        auto_approve: bool,
    },

    /// Start HTTP API server
    Server {
        /// Bind address (default: 0.0.0.0)
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Listen port (default: 8000)
        #[arg(short, long, default_value_t = 8000)]
        port: u16,

        /// Enable debug mode
        #[arg(long)]
        debug: bool,
    },

    /// Check for untranslated content in ePub files
    CheckUntranslated {
        /// Directory containing translated ePub files
        #[arg(short, long)]
        dir: PathBuf,
    },

    /// Apply manual fixes from JSON file
    ApplyFix {
        /// Path to JSON file with manual translations
        #[arg(short, long)]
        json: PathBuf,
    },
}

/// Handle Markdown translation command
pub async fn handle_md(
    file: PathBuf,
    output: Option<PathBuf>,
    source_lang: Option<String>,
    target_lang: String,
    recursive: bool,
) -> anyhow::Result<()> {
    use crate::processors::markdown::MarkdownProcessor;
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::Instant;
    use tracing::info;

    let start_time = Instant::now();

    // Determine output path
    let output = output.unwrap_or_else(|| {
        if file.is_dir() {
            file.join("translated")
        } else {
            let mut out = file.clone();
            let mut filename = file.file_name().unwrap().to_os_string();
            filename.push("_translated");
            out.set_file_name(filename);
            out
        }
    });

    info!("Starting Markdown translation");
    info!("Input: {}", file.display());
    info!("Output: {}", output.display());
    info!("Target language: {}", target_lang);
    info!("Recursive: {}", recursive);

    // Create processor
    let processor = MarkdownProcessor::from_env()?;

    // Find files
    let files = if file.is_dir() {
        if recursive {
            processor.find_files_recursive(&file)?
        } else {
            processor.find_files(&file)?
        }
    } else {
        vec![file]
    };

    if files.is_empty() {
        anyhow::bail!("No Markdown files found");
    }

    // Create progress bar
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
        .unwrap()
        .progress_chars("=>-"));

    // Process files
    let mut processed = 0;
    let mut failed = 0;

    for file_path in files {
        pb.set_message(format!("Processing: {}", file_path.display()));

        match processor
            .translate_file(&file_path, &output, &target_lang, source_lang.clone())
            .await
        {
            Ok(_) => {
                processed += 1;
                pb.inc(1);
            }
            Err(e) => {
                failed += 1;
                pb.set_message(format!("Failed: {} - {}", file_path.display(), e));
                eprintln!("Error processing {}: {}", file_path.display(), e);
            }
        }
    }

    pb.finish_with_message("Completed");

    let duration = start_time.elapsed();
    info!(
        "Completed: {} processed, {} failed in {:?}",
        processed, failed, duration
    );

    println!("\n✅ Translation completed!");
    println!("   Processed: {}", processed);
    println!("   Failed: {}", failed);
    println!("   Time: {:?}", duration);

    Ok(())
}

/// Handle ePub translation command
pub async fn handle_epub(
    file: PathBuf,
    output: PathBuf,
    source_lang: Option<String>,
    target_lang: String,
    auto_approve: bool,
) -> anyhow::Result<()> {
    use crate::processors::epub::EpubProcessor;
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::Instant;
    use tracing::info;

    let start_time = Instant::now();

    info!("Starting ePub translation");
    info!("Input: {}", file.display());
    info!("Output: {}", output.display());
    info!("Target language: {}", target_lang);
    info!("Auto-approve: {}", auto_approve);

    // Create processor
    let processor = EpubProcessor::from_env()?;

    // Find files
    let files = if file.is_dir() {
        processor.find_epub_files(&file)?
    } else {
        vec![file]
    };

    if files.is_empty() {
        anyhow::bail!("No ePub files found");
    }

    // Create progress bar
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
        .unwrap()
        .progress_chars("=>-"));

    // Process files
    let mut processed = 0;
    let mut failed = 0;

    for file_path in files {
        pb.set_message(format!("Processing: {}", file_path.display()));

        match processor
            .translate_epub(&file_path, &output, &target_lang, source_lang.clone(), auto_approve)
            .await
        {
            Ok(_) => {
                processed += 1;
                pb.inc(1);
            }
            Err(e) => {
                failed += 1;
                pb.set_message(format!("Failed: {} - {}", file_path.display(), e));
                eprintln!("Error processing {}: {}", file_path.display(), e);
            }
        }
    }

    pb.finish_with_message("Completed");

    let duration = start_time.elapsed();
    info!(
        "Completed: {} processed, {} failed in {:?}",
        processed, failed, duration
    );

    println!("\n✅ ePub translation completed!");
    println!("   Processed: {}", processed);
    println!("   Failed: {}", failed);
    println!("   Time: {:?}", duration);

    // Generate leak report if not auto-approve
    if !auto_approve {
        println!("\n📝 Generating leak report...");
        match processor.generate_leak_report(&output, &target_lang).await {
            Ok(_) => println!("   Leak report generated successfully"),
            Err(e) => eprintln!("   Failed to generate leak report: {}", e),
        }
    }

    Ok(())
}

/// Handle server command
pub async fn handle_server(host: String, port: u16, debug: bool) -> anyhow::Result<()> {
    use crate::server::api::run_server;
    use tracing::info;

    if debug {
        std::env::set_var("RUST_LOG", "debug");
    }

    info!("Starting HTTP server on {}:{}", host, port);
    println!("🚀 Server starting on http://{}:{}", host, port);
    println!("📊 API Documentation: http://{}:{}/swagger", host, port);
    println!("📄 ReDoc Documentation: http://{}:{}/redoc", host, port);

    run_server(host, port).await?;

    Ok(())
}

/// Handle check untranslated command
pub async fn handle_check_untranslated(dir: PathBuf) -> anyhow::Result<()> {
    use crate::processors::epub::EpubProcessor;
    use tracing::info;

    info!("Checking for untranslated content in: {}", dir.display());

    let processor = EpubProcessor::from_env()?;
    let leaks = processor.check_untranslated(&dir).await?;

    if leaks.is_empty() {
        println!("✅ No untranslated content found!");
        return Ok(());
    }

    println!("\n⚠️  Found {} untranslated segments:", leaks.len());

    for (i, leak) in leaks.iter().enumerate() {
        println!("\n{}. Book: {}", i + 1, leak.book_name);
        println!("   File: {}", leak.file_path);
        println!("   Original: {}", leak.original);
    }

    // Generate JSON file for manual translation
    let json_path = dir.join("人工翻译.json");
    processor.save_leak_report(&leaks, &json_path).await?;

    println!("\n📝 Manual translation file saved to: {}", json_path.display());
    println!("   Please edit this file and use 'apply-fix' command to apply translations.");

    Ok(())
}

/// Handle apply fix command
pub async fn handle_apply_fix(json: PathBuf) -> anyhow::Result<()> {
    use crate::processors::epub::EpubProcessor;
    use tracing::info;

    info!("Applying manual fixes from: {}", json.display());

    let processor = EpubProcessor::from_env()?;
    let count = processor.apply_fixes(&json).await?;

    println!("✅ Applied {} translations from {}", count, json.display());

    Ok(())
}