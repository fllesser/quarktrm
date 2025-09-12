//! Data models for Quark Drive API and application state
//!
//! This module contains all the data structures used to represent:
//! - Quark Drive API requests and responses
//! - Internal application state
//! - Task processing models
#![allow(unused)]

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Helper function to create a default UTC DateTime for serde defaults
fn default_datetime() -> DateTime<Utc> {
    Utc::now()
}

/// Represents a Quark Drive user account
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct User {
    /// User ID
    #[serde(default)]
    pub user_id: String,

    /// User name or nickname
    #[serde(default)]
    pub nickname: Option<String>,

    /// User avatar URL
    #[serde(default)]
    pub avatar_url: Option<String>,

    /// Account storage details
    #[serde(default)]
    pub storage: Option<StorageInfo>,
}

/// Storage information for a user account
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageInfo {
    /// Total storage capacity in bytes
    #[serde(default)]
    pub total_capacity: u64,

    /// Used storage in bytes
    #[serde(default)]
    pub used_capacity: u64,

    /// Remaining storage in bytes
    #[serde(default)]
    pub free_capacity: u64,
}

/// Quark Drive share information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShareInfo {
    /// Share ID
    #[serde(default)]
    pub share_id: String,

    /// Share URL
    #[serde(default)]
    pub share_url: String,

    /// Share title
    #[serde(default)]
    pub title: String,

    /// Share creator information
    #[serde(default)]
    pub creator: Option<User>,

    /// Whether the share requires an extraction code
    #[serde(default)]
    pub requires_extraction_code: bool,

    /// Whether the share is valid
    #[serde(default)]
    pub is_valid: bool,

    /// When the share expires
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Represents a file in Quark Drive
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileInfo {
    /// File ID
    #[serde(default)]
    pub file_id: String,

    /// Parent folder ID
    #[serde(default)]
    pub parent_id: String,

    /// File name
    #[serde(default)]
    pub name: String,

    /// File size in bytes
    #[serde(default)]
    pub size: u64,

    /// MIME type
    #[serde(default)]
    pub mime_type: Option<String>,

    /// File hash
    #[serde(default)]
    pub hash: Option<String>,

    /// Created time
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Modified time
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,

    /// Whether the file is a directory
    #[serde(default)]
    pub is_directory: bool,

    /// Thumbnail URL (for images and videos)
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

/// Represents a file listing with pagination
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileListResponse {
    /// List of files and folders
    #[serde(default)]
    pub items: Vec<FileInfo>,

    /// Total count of items
    #[serde(default)]
    pub total_count: u64,

    /// Continuation token for pagination
    #[serde(default)]
    pub next_marker: Option<String>,
}

/// Represents a task processing result
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskResult {
    /// Task ID
    #[serde(default)]
    pub task_id: String,

    /// Task name
    #[serde(default)]
    pub name: String,

    /// When the task started
    #[serde(default = "Utc::now")]
    pub started_at: DateTime<Utc>,

    /// When the task completed
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,

    /// Processing status
    #[serde(default)]
    pub status: TaskStatus,

    /// Result summary
    #[serde(default)]
    pub summary: TaskSummary,
}

/// Status of a task
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    /// Task is queued
    #[default]
    Queued,

    /// Task is currently running
    Running,

    /// Task completed successfully
    Completed,

    /// Task failed
    Failed,

    /// Task was skipped
    Skipped,
}

/// Summary of task results
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskSummary {
    /// Files found
    pub files_found: usize,

    /// Files processed
    pub files_processed: usize,

    /// Files saved
    pub files_saved: usize,

    /// Files skipped (already exist)
    pub files_skipped: usize,

    /// Files failed
    pub files_failed: usize,

    /// Total bytes processed
    pub bytes_processed: u64,

    /// Processing errors
    pub errors: Vec<String>,

    /// Processed file details
    pub processed_files: Vec<ProcessedFile>,
}

/// Information about a processed file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessedFile {
    /// Original file name
    #[serde(default)]
    pub original_name: String,

    /// New file name (after renaming)
    #[serde(default)]
    pub new_name: Option<String>,

    /// Destination path
    #[serde(default)]
    pub destination_path: PathBuf,

    /// File size
    #[serde(default)]
    pub size: u64,

    /// Processing status
    #[serde(default)]
    pub status: FileProcessingStatus,

    /// Error message (if any)
    #[serde(default)]
    pub error: Option<String>,
}

/// Status of file processing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FileProcessingStatus {
    /// File was saved successfully
    #[default]
    Saved,

    /// File was skipped (already exists)
    Skipped,

    /// File processing failed
    Failed,
}

/// Request body for file transfer operation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileTransferRequest {
    /// Source file ID
    #[serde(default)]
    pub file_id: String,

    /// Target folder ID
    #[serde(default)]
    pub target_folder_id: String,

    /// Optional new name for the file
    #[serde(default)]
    pub new_name: Option<String>,
}

/// Response for file transfer operation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileTransferResponse {
    /// Operation success status
    #[serde(default)]
    pub success: bool,

    /// New file ID after transfer
    #[serde(default)]
    pub new_file_id: Option<String>,

    /// Error message if operation failed
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Quark Drive API error response
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiError {
    /// Error code
    #[serde(default)]
    pub code: String,

    /// Error message
    #[serde(default)]
    pub message: String,

    /// Additional error details
    #[serde(default)]
    pub details: Option<HashMap<String, serde_json::Value>>,
}

/// Status of a share link
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ShareStatus {
    /// Share is valid and accessible
    #[default]
    Valid,

    /// Share requires an extraction code
    RequiresExtractionCode,

    /// Share has expired
    Expired,

    /// Share is invalid or deleted
    Invalid,
}

/// Application event for logging and notifications
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppEvent {
    /// When the event occurred
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,

    /// Event type
    #[serde(default)]
    pub event_type: EventType,

    /// Event message
    #[serde(default)]
    pub message: String,

    /// Associated task ID (if any)
    #[serde(default)]
    pub task_id: Option<String>,

    /// Additional event data
    #[serde(default)]
    pub data: Option<HashMap<String, serde_json::Value>>,
}

/// Types of application events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EventType {
    /// Informational event
    #[default]
    Info,

    /// Warning event
    Warning,

    /// Error event
    Error,

    /// Task started
    TaskStarted,

    /// Task completed
    TaskCompleted,

    /// Task failed
    TaskFailed,

    /// File saved
    FileSaved,

    /// Authentication
    Authentication,

    /// Media library refresh
    MediaLibraryRefresh,
}
