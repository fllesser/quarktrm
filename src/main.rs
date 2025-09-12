//! Quark Drive Auto-Save
//!
//! A command-line tool for automatically transferring files from Quark Drive shares
//! to your own Quark Drive account, with support for file renaming, scheduling, and notifications.
//!
//! Based on the concept of https://github.com/Cp0204/quark-auto-save but implemented in Rust.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use log::{error, info, warn};
use tokio::fs;

mod api;
mod config;
mod models;
mod notification;
mod tasks;
mod utils;

use api::QuarkClient;
use config::Config;
use notification::NotificationManager;
use tasks::TaskProcessor;
use utils::time_utils::execution_timer;

/// Command-line arguments for QuarkTRM
#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[clap(short, long, value_name = "FILE", default_value = "config.json")]
    config: PathBuf,

    /// Sets the log level (trace, debug, info, warn, error)
    #[clap(short, long, default_value = "info")]
    log_level: log::LevelFilter,

    /// Subcommand to execute
    #[clap(subcommand)]
    command: Commands,
}

/// Available subcommands
#[derive(Subcommand)]
enum Commands {
    /// Run tasks from configuration
    Run {
        /// Optional task ID to run (runs all scheduled tasks if not specified)
        #[clap(long)]
        task_id: Option<String>,
    },

    /// Check a share link's validity and content
    CheckShare {
        /// Share URL to check
        #[clap(long)]
        url: String,

        /// Optional extraction code for protected shares
        #[clap(long)]
        code: Option<String>,
    },

    /// Perform daily sign-in to get space rewards
    SignIn,

    /// Initialize a new configuration file
    Init {
        /// Output path for the configuration file
        #[clap(long, default_value = "config.json")]
        output: PathBuf,
    },

    /// List all tasks in the configuration
    ListTasks,

    /// Print user account information
    AccountInfo,
}

/// Main entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let cli = Cli::parse();

    // Initialize logging
    env_logger::Builder::new()
        .filter_level(cli.log_level)
        .format_timestamp_millis()
        .init();

    info!("QuarkTRM - Quark Drive Auto-Save starting...");

    // Create execution timer
    let mut timer = execution_timer();

    // Process commands
    match &cli.command {
        Commands::Run { task_id } => run_tasks(&cli.config, task_id.as_deref()).await?,
        Commands::CheckShare { url, code } => {
            check_share(&cli.config, url, code.as_deref()).await?
        }
        Commands::SignIn => daily_sign_in(&cli.config).await?,
        Commands::Init { output } => init_config(output).await?,
        Commands::ListTasks => list_tasks(&cli.config).await?,
        Commands::AccountInfo => show_account_info(&cli.config).await?,
    }

    // Log execution time
    timer.checkpoint("Command completed");
    timer.log_checkpoints();

    Ok(())
}

/// Run tasks from configuration
async fn run_tasks(config_path: &Path, task_id: Option<&str>) -> Result<()> {
    // Load configuration
    let config = load_config(config_path).await?;
    let config = Arc::new(config);

    // Initialize API client
    let client = init_api_client(&config)?;
    let client = Arc::new(client);

    // Verify authentication
    let user = client.verify_auth().await?;
    info!("Authenticated as user: {}", user.user_id);

    // Initialize notification manager
    let notification = NotificationManager::new(config.clone())?;
    let notification = Arc::new(notification);

    // Initialize task processor
    let processor = TaskProcessor::new(client.clone(), config.clone(), notification.clone());

    if let Some(id) = task_id {
        // Run a specific task
        info!("Running task: {}", id);
        let result = processor.execute_task(id).await?;

        // Display result summary
        info!("Task {} completed with status {:?}", id, result.status);
        info!(
            "Files: {} found, {} saved, {} skipped, {} failed",
            result.summary.files_found,
            result.summary.files_saved,
            result.summary.files_skipped,
            result.summary.files_failed
        );

        if !result.summary.errors.is_empty() {
            warn!("Task encountered {} errors:", result.summary.errors.len());
            for (i, error) in result.summary.errors.iter().enumerate() {
                warn!("  Error {}: {}", i + 1, error);
            }
        }
    } else {
        // Run all scheduled tasks
        info!("Running all scheduled tasks");
        let results = processor.run_all_scheduled_tasks().await;

        info!("Completed {} tasks", results.len());

        // Log a summary of all task results
        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(task_result) => {
                    info!(
                        "Task {}: {} - {:?} ({} saved, {} skipped, {} failed)",
                        i + 1,
                        task_result.name,
                        task_result.status,
                        task_result.summary.files_saved,
                        task_result.summary.files_skipped,
                        task_result.summary.files_failed
                    );
                }
                Err(err) => {
                    error!("Task {} failed: {}", i + 1, err);
                }
            }
        }
    }

    Ok(())
}

/// Check a share link's validity and content
async fn check_share(config_path: &Path, url: &str, code: Option<&str>) -> Result<()> {
    // Load configuration
    let config = load_config(config_path).await?;

    // Initialize API client
    let client = init_api_client(&config)?;

    // Check share status
    let status = client.check_share_status(url, code).await?;

    info!("Share status: {:?}", status);

    // If valid, list files in the share
    use models::ShareStatus;
    match status {
        ShareStatus::Valid => {
            info!("Listing files in share...");
            let files = client
                .list_shared_files(url, None, code, None, Some(10))
                .await?;

            info!(
                "Found {} files/folders (showing first 10):",
                files.total_count
            );
            for (i, file) in files.items.iter().enumerate() {
                let file_type = if file.is_directory { "📁" } else { "📄" };
                let size = if file.is_directory {
                    String::from("--")
                } else {
                    utils::string_utils::format_file_size(file.size)
                };

                info!("  {}. {} {} ({})", i + 1, file_type, file.name, size);
            }

            if files.total_count > 10 {
                info!("...and {} more items", files.total_count - 10);
            }
        }
        ShareStatus::RequiresExtractionCode => {
            warn!("Share requires an extraction code. Please provide one with --code.");
        }
        ShareStatus::Expired => {
            warn!("Share has expired.");
        }
        ShareStatus::Invalid => {
            warn!("Share is invalid or has been deleted.");
        }
    }

    Ok(())
}

/// Perform daily sign-in
async fn daily_sign_in(config_path: &Path) -> Result<()> {
    // Load configuration
    let config = load_config(config_path).await?;

    // Initialize API client
    let client = init_api_client(&config)?;

    // Verify authentication
    let user = client.verify_auth().await?;
    info!("Authenticated as user: {}", user.user_id);

    // Perform sign-in
    let result = client.daily_sign_in().await?;

    if result {
        info!("Daily sign-in successful!");
    } else {
        warn!("Daily sign-in failed. You may have already signed in today.");
    }

    Ok(())
}

/// Initialize a new configuration file
async fn init_config(output_path: &Path) -> Result<()> {
    // Check if file already exists
    if output_path.exists() {
        return Err(anyhow!(
            "Configuration file already exists: {}",
            output_path.display()
        ));
    }

    // Create a default configuration
    let config = Config::default();

    // Save to file
    let config_json =
        serde_json::to_string_pretty(&config).context("Failed to serialize configuration")?;

    fs::write(output_path, config_json)
        .await
        .with_context(|| format!("Failed to write configuration to {}", output_path.display()))?;

    info!("Created new configuration file: {}", output_path.display());
    info!("Please edit this file to add your Quark Drive authentication and task configuration.");

    Ok(())
}

/// List all tasks in the configuration
async fn list_tasks(config_path: &Path) -> Result<()> {
    // Load configuration
    let config = load_config(config_path).await?;

    info!("Found {} tasks in configuration:", config.tasks.len());

    for (i, task) in config.tasks.iter().enumerate() {
        let status = if config.should_run_today(&task.id) {
            "Active"
        } else {
            "Inactive"
        };

        info!("{}. {} ({})", i + 1, task.name, status);
        info!("   ID: {}", task.id);
        info!("   Target directory: {}", task.target_directory.display());
        info!("   Subtasks: {}", task.subtasks.len());

        if let Some(end_date) = task.end_date {
            info!("   End date: {}", end_date);
        }

        info!("");
    }

    Ok(())
}

/// Show account information
async fn show_account_info(config_path: &Path) -> Result<()> {
    // Load configuration
    let config = load_config(config_path).await?;

    // Initialize API client
    let client = init_api_client(&config)?;

    // Get user info
    let user = client.get_user_info().await?;

    info!("User ID: {}", user.user_id);

    if let Some(nickname) = &user.nickname {
        info!("Nickname: {}", nickname);
    }

    if let Some(storage) = &user.storage {
        let used_percent = (storage.used_capacity as f64 / storage.total_capacity as f64) * 100.0;

        info!("Storage:");
        info!(
            "  Total: {}",
            utils::string_utils::format_file_size(storage.total_capacity)
        );
        info!(
            "  Used: {} ({:.1}%)",
            utils::string_utils::format_file_size(storage.used_capacity),
            used_percent
        );
        info!(
            "  Free: {}",
            utils::string_utils::format_file_size(storage.free_capacity)
        );
    }

    Ok(())
}

/// Load configuration from file
async fn load_config(config_path: &Path) -> Result<Config> {
    if !config_path.exists() {
        return Err(anyhow!(
            "Configuration file not found: {}. Run 'init' command to create one.",
            config_path.display()
        ));
    }

    let config_str = fs::read_to_string(config_path)
        .await
        .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

    let config: Config = serde_json::from_str(&config_str)
        .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

    Ok(config)
}

/// Initialize API client from configuration
fn init_api_client(config: &Config) -> Result<QuarkClient> {
    QuarkClient::new(
        config.auth.cookie.clone(),
        config.auth.access_token.clone(),
        config.auth.user_agent.clone(),
    )
}
