//! Configuration module for Quark Drive Auto-Save
//!
//! This module handles loading, parsing, and providing access to user configuration.
//! Configuration can be loaded from a file or environment variables.
#![allow(unused)]

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Utc, Weekday as ChronoWeekday};
use log::{info, warn};
use serde::{Deserialize, Serialize};

/// Represents days of the week for task scheduling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    /// Converts a chrono Weekday to our Weekday enum
    pub fn from_chrono_weekday(weekday: ChronoWeekday) -> Self {
        match weekday {
            ChronoWeekday::Mon => Weekday::Monday,
            ChronoWeekday::Tue => Weekday::Tuesday,
            ChronoWeekday::Wed => Weekday::Wednesday,
            ChronoWeekday::Thu => Weekday::Thursday,
            ChronoWeekday::Fri => Weekday::Friday,
            ChronoWeekday::Sat => Weekday::Saturday,
            ChronoWeekday::Sun => Weekday::Sunday,
        }
    }
}

/// Authentication configuration for Quark Drive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// User's cookie string for authentication
    pub cookie: String,

    /// User's access token, if available
    #[serde(default)]
    pub access_token: Option<String>,

    /// Optional user agent string to use in requests
    #[serde(default)]
    pub user_agent: Option<String>,
}

/// Configuration for a single subtask within a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTaskConfig {
    /// Share URL to process
    pub share_url: String,

    /// Extraction code for protected shares (if needed)
    #[serde(default)]
    pub extraction_code: Option<String>,

    /// Optional subdirectory within the share to process
    #[serde(default)]
    pub subdirectory: Option<String>,

    /// Regular expression pattern to match files for processing
    pub file_pattern: String,

    /// Replacement pattern for renaming files
    #[serde(default)]
    pub rename_pattern: Option<String>,

    /// Whether to ignore file extensions during pattern matching
    #[serde(default)]
    pub ignore_extension: bool,

    /// Days of the week this task should run
    #[serde(default)]
    pub run_on_days: Option<Vec<Weekday>>,

    /// Custom tags or metadata for the subtask
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

/// Configuration for a task that processes one or more share URLs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Unique identifier for the task
    pub id: String,

    /// Human-readable name for the task
    pub name: String,

    /// Target directory to save files to
    pub target_directory: PathBuf,

    /// Whether to create the target directory if it doesn't exist
    #[serde(default = "default_true")]
    pub create_directory: bool,

    /// Optional end date after which the task will no longer run
    #[serde(default)]
    pub end_date: Option<DateTime<Utc>>,

    /// List of subtasks to process
    pub subtasks: Vec<SubTaskConfig>,

    /// Whether to enable notifications for this task
    #[serde(default = "default_true")]
    pub enable_notifications: bool,

    /// Whether to refresh media libraries after this task
    #[serde(default)]
    pub refresh_media_library: bool,
}

/// Returns a default value of true for boolean configuration options
fn default_true() -> bool {
    true
}

/// Notification configuration settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    /// Whether notifications are enabled globally
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Pushover notification settings
    #[serde(default)]
    pub pushover: Option<PushoverConfig>,

    /// Telegram notification settings
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,

    /// Discord webhook notification settings
    #[serde(default)]
    pub discord: Option<DiscordConfig>,

    /// Custom webhook notification settings
    #[serde(default)]
    pub webhook: Option<WebhookConfig>,
}

/// Pushover notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushoverConfig {
    /// Pushover API token
    pub api_token: String,

    /// Pushover user key
    pub user_key: String,
}

/// Telegram notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Telegram bot token
    pub bot_token: String,

    /// Telegram chat ID
    pub chat_id: String,
}

/// Discord webhook notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Discord webhook URL
    pub webhook_url: String,
}

/// Custom webhook notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Webhook URL
    pub url: String,

    /// HTTP method to use (GET, POST, etc.)
    #[serde(default = "default_post_method")]
    pub method: String,

    /// Optional HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Optional request body template
    #[serde(default)]
    pub body_template: Option<String>,
}

/// Returns the default HTTP method (POST) for webhook configurations
fn default_post_method() -> String {
    "POST".to_string()
}

/// Media library integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaLibraryConfig {
    /// Emby server configuration
    #[serde(default)]
    pub emby: Option<EmbyConfig>,

    /// Jellyfin server configuration
    #[serde(default)]
    pub jellyfin: Option<JellyfinConfig>,

    /// Plex server configuration
    #[serde(default)]
    pub plex: Option<PlexConfig>,
}

/// Emby server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbyConfig {
    /// Emby server URL
    pub server_url: String,

    /// Emby API key
    pub api_key: String,

    /// Optional list of library IDs to refresh
    #[serde(default)]
    pub library_ids: Vec<String>,
}

/// Jellyfin server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JellyfinConfig {
    /// Jellyfin server URL
    pub server_url: String,

    /// Jellyfin API key
    pub api_key: String,

    /// Optional list of library IDs to refresh
    #[serde(default)]
    pub library_ids: Vec<String>,
}

/// Plex server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlexConfig {
    /// Plex server URL
    pub server_url: String,

    /// Plex token
    pub token: String,

    /// Optional list of library section IDs to refresh
    #[serde(default)]
    pub library_section_ids: Vec<String>,
}

/// Main configuration structure for the application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Authentication configuration
    pub auth: AuthConfig,

    /// List of tasks to process
    pub tasks: Vec<TaskConfig>,

    /// Notification configuration
    #[serde(default)]
    pub notification: NotificationConfig,

    /// Media library integration configuration
    #[serde(default)]
    pub media_library: Option<MediaLibraryConfig>,

    /// Global configuration options
    #[serde(default)]
    pub options: GlobalOptions,
}

/// Global configuration options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalOptions {
    /// Directory to store application data
    #[serde(default)]
    pub data_dir: Option<PathBuf>,

    /// Whether to enable daily sign-in
    #[serde(default)]
    pub enable_daily_signin: bool,

    /// Custom user agent string
    #[serde(default)]
    pub user_agent: Option<String>,

    /// Maximum concurrent tasks
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,

    /// Custom configuration variables
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

/// Returns the default maximum number of concurrent tasks
fn default_max_concurrent_tasks() -> usize {
    2
}

/// Returns the default value for GlobalOptions
impl Default for GlobalOptions {
    fn default() -> Self {
        Self {
            data_dir: None,
            enable_daily_signin: false,
            user_agent: None,
            max_concurrent_tasks: default_max_concurrent_tasks(),
            variables: HashMap::new(),
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config_str = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        let config: Config = serde_json::from_str(&config_str)
            .with_context(|| format!("Failed to parse config file: {}", path.as_ref().display()))?;

        info!("Loaded configuration from {}", path.as_ref().display());
        Ok(config)
    }

    /// Create a default configuration
    pub fn default() -> Self {
        Config {
            auth: AuthConfig {
                cookie: String::new(),
                access_token: None,
                user_agent: None,
            },
            tasks: Vec::new(),
            notification: NotificationConfig {
                enabled: true,
                pushover: None,
                telegram: None,
                discord: None,
                webhook: None,
            },
            media_library: None,
            options: GlobalOptions::default(),
        }
    }

    /// Save configuration to a file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let config_str =
            serde_json::to_string_pretty(self).context("Failed to serialize configuration")?;

        fs::write(&path, config_str).with_context(|| {
            format!(
                "Failed to write config to file: {}",
                path.as_ref().display()
            )
        })?;

        info!("Saved configuration to {}", path.as_ref().display());
        Ok(())
    }

    /// Check if a task should run today
    pub fn should_run_today(&self, task_id: &str) -> bool {
        let task = match self.tasks.iter().find(|t| t.id == task_id) {
            Some(t) => t,
            None => {
                warn!("Task with ID {} not found", task_id);
                return false;
            }
        };

        // Check if the task has an end date and if it has passed
        if let Some(end_date) = task.end_date {
            if end_date < Utc::now() {
                info!("Task {} has expired (end date: {})", task_id, end_date);
                return false;
            }
        }

        true
    }

    /// Check if a subtask should run today
    pub fn should_run_subtask_today(&self, task_id: &str, subtask_index: usize) -> bool {
        let task = match self.tasks.iter().find(|t| t.id == task_id) {
            Some(t) => t,
            None => {
                warn!("Task with ID {} not found", task_id);
                return false;
            }
        };

        if !self.should_run_today(task_id) {
            return false;
        }

        let subtask = match task.subtasks.get(subtask_index) {
            Some(s) => s,
            None => {
                warn!(
                    "Subtask index {} not found in task {}",
                    subtask_index, task_id
                );
                return false;
            }
        };

        // If run_on_days is specified, check if today is one of those days
        if let Some(run_days) = &subtask.run_on_days {
            let now = Utc::now();
            let today = Weekday::from_chrono_weekday(now.weekday());
            if !run_days.contains(&today) {
                info!(
                    "Subtask {} of task {} is not scheduled to run today",
                    subtask_index, task_id
                );
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_config() {
        let config_json = r#"
        {
            "auth": {
                "cookie": "example_cookie_string"
            },
            "tasks": [
                {
                    "id": "task1",
                    "name": "My First Task",
                    "target_directory": "/path/to/save",
                    "subtasks": [
                        {
                            "share_url": "https://pan.quark.cn/s/share-link",
                            "file_pattern": ".*\\.mp4$"
                        }
                    ]
                }
            ]
        }
        "#;

        let config: Config = serde_json::from_str(config_json).unwrap();
        assert_eq!(config.auth.cookie, "example_cookie_string");
        assert_eq!(config.tasks.len(), 1);
        assert_eq!(config.tasks[0].id, "task1");
        assert_eq!(config.tasks[0].subtasks.len(), 1);
        assert_eq!(
            config.tasks[0].subtasks[0].share_url,
            "https://pan.quark.cn/s/share-link"
        );
    }
}
