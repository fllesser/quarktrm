//! Task processing module for Quark Drive Auto-Save
//!
//! This module handles the execution of file transfer tasks, including:
//! - Processing share links
//! - Transferring files based on regex patterns
//! - Renaming files according to configured patterns
//! - Generating task summaries

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use log::{debug, error, info};
use regex::Regex;
use tokio::sync::{Mutex, Semaphore};
use tokio::task;

use crate::api::QuarkClient;
use crate::config::{Config, SubTaskConfig, TaskConfig};
use crate::models::{
    FileInfo, FileProcessingStatus, ProcessedFile, ShareStatus, TaskResult, TaskStatus, TaskSummary,
};
use crate::notification::NotificationManager;

/// A task processor that manages and executes file transfer tasks
#[derive(Clone)]
pub struct TaskProcessor {
    /// Quark Drive API client
    client: Arc<QuarkClient>,

    /// Application configuration
    config: Arc<Config>,

    /// Notification manager for sending alerts
    notification: Arc<NotificationManager>,

    /// Semaphore to limit concurrent operations
    concurrency_limiter: Arc<Semaphore>,

    /// Set of already processed file IDs to prevent duplication
    processed_files: Arc<Mutex<HashSet<String>>>,

    /// Cache of folder IDs to avoid redundant API calls
    folder_id_cache: Arc<Mutex<HashMap<String, String>>>,
}

impl TaskProcessor {
    /// Create a new task processor
    pub fn new(
        client: Arc<QuarkClient>,
        config: Arc<Config>,
        notification: Arc<NotificationManager>,
    ) -> Self {
        // Ensure we have at least 1 concurrent task allowed
        let max_concurrent = std::cmp::max(1, config.options.max_concurrent_tasks);

        Self {
            client,
            config,
            notification,
            concurrency_limiter: Arc::new(Semaphore::new(max_concurrent)),
            processed_files: Arc::new(Mutex::new(HashSet::new())),
            folder_id_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Execute a task by ID
    pub async fn execute_task(&self, task_id: &str) -> Result<TaskResult> {
        // Find the task in configuration
        let task = self
            .config
            .tasks
            .iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| anyhow!("Task not found: {}", task_id))?
            .clone();

        // Check if task should run today
        if !self.config.should_run_today(task_id) {
            info!("Task {} is not scheduled to run today, skipping", task_id);
            return Ok(TaskResult {
                task_id: task_id.to_string(),
                name: task.name.clone(),
                started_at: Utc::now(),
                completed_at: Some(Utc::now()),
                status: TaskStatus::Skipped,
                summary: TaskSummary::default(),
            });
        }

        // Start task execution
        let start_time = Utc::now();
        let processing_start = Instant::now();

        info!("Starting task: {} ({})", task.name, task_id);

        // Notify task start
        if task.enable_notifications {
            self.notification
                .send_notification(
                    &format!("Task started: {}", task.name),
                    &format!(
                        "Processing task {} with {} subtasks",
                        task.name,
                        task.subtasks.len()
                    ),
                    Some(task_id),
                )
                .await;
        }

        // Initialize task result
        let mut task_result = TaskResult {
            task_id: task_id.to_string(),
            name: task.name.clone(),
            started_at: start_time,
            completed_at: None,
            status: TaskStatus::Running,
            summary: TaskSummary::default(),
        };

        // Create target directory if needed
        if task.create_directory {
            if !task.target_directory.exists() {
                info!(
                    "Creating target directory: {}",
                    task.target_directory.display()
                );
                std::fs::create_dir_all(&task.target_directory)
                    .context("Failed to create target directory")?;
            }
        } else if !task.target_directory.exists() {
            return Err(anyhow!(
                "Target directory does not exist: {}",
                task.target_directory.display()
            ));
        }

        // Process subtasks
        for (idx, subtask) in task.subtasks.iter().enumerate() {
            // Check if subtask should run today
            if !self.config.should_run_subtask_today(task_id, idx) {
                info!(
                    "Subtask {} of task {} is not scheduled to run today, skipping",
                    idx, task_id
                );
                continue;
            }

            match self.process_subtask(&task, subtask).await {
                Ok(subtask_summary) => {
                    // Merge subtask summary into task summary
                    task_result.summary.files_found += subtask_summary.files_found;
                    task_result.summary.files_processed += subtask_summary.files_processed;
                    task_result.summary.files_saved += subtask_summary.files_saved;
                    task_result.summary.files_skipped += subtask_summary.files_skipped;
                    task_result.summary.files_failed += subtask_summary.files_failed;
                    task_result.summary.bytes_processed += subtask_summary.bytes_processed;
                    task_result
                        .summary
                        .processed_files
                        .extend(subtask_summary.processed_files);
                    task_result.summary.errors.extend(subtask_summary.errors);
                }
                Err(err) => {
                    let error_msg = format!(
                        "Error processing subtask for share {}: {}",
                        subtask.share_url, err
                    );
                    error!("{}", error_msg);
                    task_result.summary.errors.push(error_msg);
                }
            }
        }

        // Update task status based on results
        task_result.completed_at = Some(Utc::now());

        if task_result.summary.errors.is_empty() {
            task_result.status = TaskStatus::Completed;
        } else {
            task_result.status = TaskStatus::Failed;
        }

        // Log completion time
        let duration = processing_start.elapsed();
        info!(
            "Task {} completed in {:.2}s with status: {:?}",
            task_id,
            duration.as_secs_f64(),
            task_result.status
        );

        // Send notification about task completion
        if task.enable_notifications {
            let status_text = match task_result.status {
                TaskStatus::Completed => "completed successfully",
                TaskStatus::Failed => "failed",
                _ => "completed with warnings",
            };

            let message = format!(
                "{}: {} files saved, {} skipped, {} failed",
                status_text,
                task_result.summary.files_saved,
                task_result.summary.files_skipped,
                task_result.summary.files_failed
            );

            self.notification
                .send_notification(
                    &format!("Task {}: {}", task.name, status_text),
                    &message,
                    Some(task_id),
                )
                .await;
        }

        // TODO: If configured, refresh media library

        Ok(task_result)
    }

    /// Process a single subtask within a task
    async fn process_subtask(
        &self,
        task: &TaskConfig,
        subtask: &SubTaskConfig,
    ) -> Result<TaskSummary> {
        let mut summary = TaskSummary::default();

        info!(
            "Processing subtask: {} with pattern: {}",
            subtask.share_url, subtask.file_pattern
        );

        // Check share status
        let share_status = self
            .client
            .check_share_status(&subtask.share_url, subtask.extraction_code.as_deref())
            .await?;

        match share_status {
            ShareStatus::Valid => {
                // Continue processing
            }
            ShareStatus::RequiresExtractionCode => {
                return Err(anyhow!(
                    "Share requires extraction code, but none provided: {}",
                    subtask.share_url
                ));
            }
            ShareStatus::Expired => {
                return Err(anyhow!("Share has expired: {}", subtask.share_url));
            }
            ShareStatus::Invalid => {
                return Err(anyhow!(
                    "Share is invalid or deleted: {}",
                    subtask.share_url
                ));
            }
        }

        // Get target folder ID (create if needed)
        let target_folder_id = match self.get_or_create_folder_id(&task.target_directory).await {
            Ok(id) => id,
            Err(err) => {
                return Err(anyhow!(
                    "Failed to get target folder ID for {}: {}",
                    task.target_directory.display(),
                    err
                ));
            }
        };

        // Build file pattern regex
        let file_pattern = build_file_pattern(&subtask.file_pattern, &task.name)?;

        // Process the share and its files
        self.process_shared_files(
            task,
            subtask,
            &file_pattern,
            &target_folder_id,
            &subtask.share_url,
            subtask.subdirectory.as_deref(),
            None,
            &mut summary,
        )
        .await?;

        Ok(summary)
    }

    /// Process files in a shared folder recursively
    #[allow(clippy::too_many_arguments)]
    async fn process_shared_files(
        &self,
        task: &TaskConfig,
        subtask: &SubTaskConfig,
        file_pattern: &Regex,
        target_folder_id: &str,
        share_url: &str,
        subdirectory: Option<&str>,
        parent_id: Option<&str>,
        summary: &mut TaskSummary,
    ) -> Result<()> {
        let mut marker = None;

        loop {
            // List files in the shared directory
            let files = self
                .client
                .list_shared_files(
                    share_url,
                    parent_id,
                    subtask.extraction_code.as_deref(),
                    marker.as_deref(),
                    Some(100),
                )
                .await?;

            summary.files_found += files.items.len();

            // Process all files and folders
            for file in &files.items {
                // Skip processing if this file has a subdirectory constraint and doesn't match
                if let Some(subdir) = subdirectory {
                    // TODO: Implement subdirectory path matching logic
                    // For now, a simple check if the file is in the right directory
                    if !file.name.contains(subdir) && parent_id.is_none() {
                        continue;
                    }
                }

                if file.is_directory {
                    // Recursively process subfolders
                    // let task = task.clone();
                    // let subtask = subtask.clone();
                    // let file_pattern = file_pattern.clone();
                    // let target_folder_id = target_folder_id.to_string();
                    // let share_url = share_url.to_string();
                    // let subdirectory = subdirectory.map(|s| s.to_string());
                    // let file_id = file.file_id.clone();
                    // let this = self.clone();

                    // Use Box::pin to handle recursive async calls
                    // Box::pin(async move {
                    //     this.process_shared_files(
                    //         &task,
                    //         &subtask,
                    //         &file_pattern,
                    //         &target_folder_id,
                    //         &share_url,
                    //         subdirectory.as_deref(),
                    //         Some(&file_id),
                    //         &mut summary,
                    //     )
                    //     .await
                    // })
                    // .await?;
                } else {
                    // Process individual file
                    let should_process =
                        should_process_file(file, file_pattern, subtask.ignore_extension);

                    if should_process {
                        // Get semaphore permit to limit concurrent transfers
                        let permit = self.concurrency_limiter.clone().acquire_owned().await?;

                        // Create clones for async task
                        let client = self.client.clone();
                        let processed_files = self.processed_files.clone();
                        let share_url = share_url.to_string();
                        let target_folder_id = target_folder_id.to_string();
                        let extraction_code = subtask.extraction_code.clone();
                        let rename_pattern = subtask.rename_pattern.clone();
                        let file_clone = file.clone();
                        let task_name = task.name.clone();

                        // Process file in a separate task
                        let processed_file = task::spawn(async move {
                            // Check if we've already processed this file
                            let file_key = format!(
                                "{}:{}",
                                file_clone.file_id,
                                file_clone.hash.as_deref().unwrap_or("")
                            );
                            {
                                let processed = processed_files.lock().await;
                                if processed.contains(&file_key) {
                                    debug!("Skipping already processed file: {}", file_clone.name);
                                    drop(permit);
                                    return Ok(ProcessedFile {
                                        original_name: file_clone.name,
                                        new_name: None,
                                        destination_path: PathBuf::new(),
                                        size: file_clone.size,
                                        status: FileProcessingStatus::Skipped,
                                        error: None,
                                    });
                                }
                            }

                            // Determine new file name if renaming is requested
                            let new_name = if let Some(pattern) = &rename_pattern {
                                rename_file(&file_clone.name, pattern, &task_name)?
                            } else {
                                None
                            };

                            // Transfer the file
                            let result = client
                                .transfer_file_from_share(
                                    &share_url,
                                    &file_clone.file_id,
                                    &target_folder_id,
                                    new_name.as_deref(),
                                    extraction_code.as_deref(),
                                )
                                .await;

                            // Create a processed file record
                            let processed_file = match result {
                                Ok(transfer) => {
                                    if transfer.success {
                                        info!(
                                            "Transferred file: {} -> {}",
                                            file_clone.name,
                                            new_name.as_deref().unwrap_or(&file_clone.name)
                                        );

                                        // Mark as processed
                                        {
                                            let mut processed = processed_files.lock().await;
                                            processed.insert(file_key);
                                        }

                                        ProcessedFile {
                                            original_name: file_clone.name,
                                            new_name,
                                            destination_path: PathBuf::new(), // We don't have actual filesystem path here
                                            size: file_clone.size,
                                            status: FileProcessingStatus::Saved,
                                            error: None,
                                        }
                                    } else {
                                        error!(
                                            "Failed to transfer file {}: {}",
                                            file_clone.name,
                                            transfer
                                                .error_message
                                                .as_ref()
                                                .map_or("Unknown error".to_string(), |s| s.clone())
                                        );

                                        ProcessedFile {
                                            original_name: file_clone.name,
                                            new_name,
                                            destination_path: PathBuf::new(),
                                            size: file_clone.size,
                                            status: FileProcessingStatus::Failed,
                                            error: transfer.error_message.clone(),
                                        }
                                    }
                                }
                                Err(err) => {
                                    error!("Error transferring file {}: {}", file_clone.name, err);

                                    ProcessedFile {
                                        original_name: file_clone.name,
                                        new_name,
                                        destination_path: PathBuf::new(),
                                        size: file_clone.size,
                                        status: FileProcessingStatus::Failed,
                                        error: Some(err.to_string()),
                                    }
                                }
                            };

                            drop(permit);
                            Ok(processed_file)
                        })
                        .await
                        .unwrap_or_else(|e| Err(anyhow!("Task join error: {}", e)))?;

                        // Update summary based on the processed file
                        summary.files_processed += 1;
                        match processed_file.status {
                            FileProcessingStatus::Saved => {
                                summary.files_saved += 1;
                                summary.bytes_processed += processed_file.size;
                            }
                            FileProcessingStatus::Skipped => summary.files_skipped += 1,
                            FileProcessingStatus::Failed => {
                                summary.files_failed += 1;
                                if let Some(err) = &processed_file.error {
                                    summary.errors.push(format!(
                                        "Failed to process file {}: {}",
                                        processed_file.original_name, err
                                    ));
                                }
                            }
                        }
                        summary.processed_files.push(processed_file);
                    } else {
                        debug!("Skipping file that doesn't match pattern: {}", file.name);
                    }
                }
            }

            // Update pagination marker for next page
            if let Some(next_marker) = &files.next_marker {
                if next_marker.is_empty() {
                    break;
                }
                marker = Some(next_marker.clone());
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Get or create a folder ID for the target directory
    async fn get_or_create_folder_id(&self, path: &Path) -> Result<String> {
        let path_str = path.to_string_lossy().to_string();

        // Check cache first
        {
            let cache = self.folder_id_cache.lock().await;
            if let Some(id) = cache.get(&path_str) {
                return Ok(id.clone());
            }
        }

        // Get or create the folder
        let folder_id = self.client.find_or_create_folder_path(&path_str).await?;

        // Update cache
        {
            let mut cache = self.folder_id_cache.lock().await;
            cache.insert(path_str, folder_id.clone());
        }

        Ok(folder_id)
    }

    /// Run all tasks that are scheduled for today
    pub async fn run_all_scheduled_tasks(&self) -> Vec<Result<TaskResult>> {
        let mut results = Vec::new();

        for task in &self.config.tasks {
            if self.config.should_run_today(&task.id) {
                match self.execute_task(&task.id).await {
                    Ok(result) => results.push(Ok(result)),
                    Err(err) => {
                        error!("Error executing task {}: {}", task.id, err);
                        results.push(Err(err));
                    }
                }
            } else {
                info!("Task {} is not scheduled to run today, skipping", task.id);
            }
        }

        results
    }
}

/// Check if a file should be processed based on the regex pattern
fn should_process_file(file: &FileInfo, pattern: &Regex, ignore_extension: bool) -> bool {
    if file.is_directory {
        return false;
    }

    if ignore_extension {
        // Extract the filename without extension for matching
        if let Some(name) = Path::new(&file.name).file_stem() {
            pattern.is_match(&name.to_string_lossy())
        } else {
            pattern.is_match(&file.name)
        }
    } else {
        pattern.is_match(&file.name)
    }
}

/// Build a regex pattern for file matching
fn build_file_pattern(pattern: &str, _task_name: &str) -> Result<Regex> {
    // Handle magic patterns
    let pattern = if pattern.starts_with('$') {
        match pattern {
            "$TV" => r"(?i)\.(mp4|mkv|avi|mov|flv|wmv|ts|m4v)$".to_string(),
            "$MOVIE" => r"(?i)\.(mp4|mkv|avi|mov|flv|wmv|ts|m4v)$".to_string(),
            "$VIDEO" => r"(?i)\.(mp4|mkv|avi|mov|flv|wmv|ts|m4v|webm)$".to_string(),
            "$AUDIO" => r"(?i)\.(mp3|flac|aac|ogg|wav|wma|m4a)$".to_string(),
            "$IMAGE" => r"(?i)\.(jpg|jpeg|png|gif|bmp|webp|tiff|svg)$".to_string(),
            "$DOCUMENT" => r"(?i)\.(pdf|doc|docx|xls|xlsx|ppt|pptx|txt|rtf|md)$".to_string(),
            "$ARCHIVE" => r"(?i)\.(zip|rar|7z|tar|gz|bz2|xz)$".to_string(),
            "$SUBTITLE" => r"(?i)\.(srt|ass|ssa|vtt|sub)$".to_string(),
            "$ALL" => r".*".to_string(),
            _ => pattern.to_string(),
        }
    } else {
        pattern.to_string()
    };

    Regex::new(&pattern).with_context(|| format!("Invalid regex pattern: {}", pattern))
}

/// Rename a file based on a regex pattern
fn rename_file(filename: &str, pattern: &str, task_name: &str) -> Result<Option<String>> {
    // If the pattern is empty, don't rename
    if pattern.is_empty() {
        return Ok(None);
    }

    // Check if this is a regex replacement or direct name
    if pattern.contains('\\') || pattern.contains('$') || pattern.contains('{') {
        // This is a regex replacement pattern
        let file_pattern = match build_file_pattern(".*", task_name) {
            Ok(pattern) => pattern,
            Err(_) => return Ok(None),
        };

        let renamed = if pattern.contains('{') {
            // Handle magic variables
            let mut new_name = pattern.to_string();

            // Replace {TASKNAME} with the task name
            new_name = new_name.replace("{TASKNAME}", task_name);

            // Replace other variables as needed
            // TODO: Add more magic variables

            // Apply regular regex replacement
            file_pattern.replace(filename, &new_name).to_string()
        } else {
            // Standard regex replacement
            file_pattern.replace(filename, pattern).to_string()
        };

        if renamed != filename {
            Ok(Some(renamed))
        } else {
            Ok(None)
        }
    } else {
        // Direct name replacement
        Ok(Some(pattern.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_file_pattern() {
        // Test regular pattern
        let pattern = build_file_pattern(r"\.mp4$", "Test").unwrap();
        assert!(pattern.is_match("video.mp4"));
        assert!(!pattern.is_match("video.mkv"));

        // Test magic pattern
        let pattern = build_file_pattern("$VIDEO", "Test").unwrap();
        assert!(pattern.is_match("video.mp4"));
        assert!(pattern.is_match("video.mkv"));
        assert!(pattern.is_match("video.avi"));
        assert!(!pattern.is_match("image.jpg"));
    }

    #[test]
    fn test_rename_file() {
        // Test regex replacement
        let result = rename_file("test-01.mp4", r"S01E\1.mp4", "Show").unwrap();
        assert_eq!(result, Some("S01E01.mp4".to_string()));

        // Test with magic variables
        let result = rename_file("01.mp4", "{TASKNAME}.S01E$1.mp4", "Show").unwrap();
        assert_eq!(result, Some("Show.S01E01.mp4".to_string()));

        // Test with no changes
        let result = rename_file("test.mp4", "", "Show").unwrap();
        assert_eq!(result, None);
    }
}
