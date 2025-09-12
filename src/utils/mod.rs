//! Utility functions and helpers for Quark Drive Auto-Save
//!
//! This module contains various utility functions used throughout the application,
//! including file handling, regex processing, error handling, and more.
#![allow(unused)]
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use log::{error, info};
use regex::Regex;

/// Regex utility functions
pub mod regex_utils {
    use super::*;

    /// Apply a regex transformation to a string
    pub fn apply_regex(input: &str, pattern: &str, replacement: &str) -> Result<String> {
        let re =
            Regex::new(pattern).with_context(|| format!("Invalid regex pattern: {}", pattern))?;

        Ok(re.replace_all(input, replacement).to_string())
    }

    /// Check if a string matches a regex pattern
    pub fn matches_pattern(input: &str, pattern: &str) -> Result<bool> {
        let re =
            Regex::new(pattern).with_context(|| format!("Invalid regex pattern: {}", pattern))?;

        Ok(re.is_match(input))
    }

    /// Extract captures from a string using a regex pattern
    pub fn extract_captures<'a>(input: &'a str, pattern: &str) -> Result<Vec<&'a str>> {
        let re =
            Regex::new(pattern).with_context(|| format!("Invalid regex pattern: {}", pattern))?;

        let caps = match re.captures(input) {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let mut results = Vec::new();
        for i in 1..caps.len() {
            if let Some(m) = caps.get(i) {
                results.push(m.as_str());
            }
        }

        Ok(results)
    }
}

/// File system utility functions
pub mod fs_utils {
    use super::*;

    /// Check if a file exists and has the same size
    pub fn file_exists_with_size<P: AsRef<Path>>(path: P, size: u64) -> bool {
        match fs::metadata(path) {
            Ok(metadata) => metadata.is_file() && metadata.len() == size,
            Err(_) => false,
        }
    }

    /// Create all parent directories for a path
    pub fn ensure_parent_dirs<P: AsRef<Path>>(path: P) -> Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }
        }
        Ok(())
    }

    /// Get all files in a directory (recursively) that match a pattern
    pub fn find_files<P: AsRef<Path>>(dir: P, pattern: &str) -> Result<Vec<PathBuf>> {
        let re =
            Regex::new(pattern).with_context(|| format!("Invalid regex pattern: {}", pattern))?;

        let mut results = Vec::new();

        for entry in walkdir::WalkDir::new(dir) {
            let entry = entry.context("Failed to access directory entry")?;
            let path = entry.path();

            if path.is_file() {
                if let Some(filename) = path.file_name() {
                    if let Some(filename_str) = filename.to_str() {
                        if re.is_match(filename_str) {
                            results.push(path.to_path_buf());
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Get a unique filename by appending a number if the file already exists
    pub fn get_unique_filename<P: AsRef<Path>>(path: P) -> PathBuf {
        let path = path.as_ref();

        if !path.exists() {
            return path.to_path_buf();
        }

        let file_stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");

        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

        let parent = path.parent().unwrap_or(Path::new(""));

        let mut counter = 1;
        loop {
            let new_filename = if extension.is_empty() {
                format!("{} ({})", file_stem, counter)
            } else {
                format!("{} ({}).{}", file_stem, counter, extension)
            };

            let new_path = parent.join(new_filename);
            if !new_path.exists() {
                return new_path;
            }

            counter += 1;
        }
    }
}

/// Time and date utility functions
pub mod time_utils {
    use super::*;

    /// Format a datetime in a human-readable format
    pub fn format_datetime(dt: &DateTime<Utc>) -> String {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// Format a duration in a human-readable format
    pub fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();

        if total_secs < 60 {
            return format!("{} seconds", total_secs);
        }

        let minutes = total_secs / 60;
        let seconds = total_secs % 60;

        if minutes < 60 {
            return format!("{}m {}s", minutes, seconds);
        }

        let hours = minutes / 60;
        let minutes = minutes % 60;

        format!("{}h {}m {}s", hours, minutes, seconds)
    }

    /// Get the current time as a formatted string
    pub fn now_string() -> String {
        format_datetime(&Utc::now())
    }

    /// Create a simple execution timer
    pub fn execution_timer() -> ExecutionTimer {
        ExecutionTimer::new()
    }

    /// A simple timer for measuring execution time
    pub struct ExecutionTimer {
        start: Instant,
        checkpoints: Vec<(String, Duration)>,
    }

    impl ExecutionTimer {
        /// Create a new execution timer
        pub fn new() -> Self {
            Self {
                start: Instant::now(),
                checkpoints: Vec::new(),
            }
        }

        /// Add a checkpoint with a label
        pub fn checkpoint(&mut self, label: &str) {
            let elapsed = self.start.elapsed();
            self.checkpoints.push((label.to_string(), elapsed));
        }

        /// Get the total elapsed time
        pub fn elapsed(&self) -> Duration {
            self.start.elapsed()
        }

        /// Get the elapsed time as a formatted string
        pub fn elapsed_str(&self) -> String {
            format_duration(self.elapsed())
        }

        /// Print all checkpoints to the log
        pub fn log_checkpoints(&self) {
            if self.checkpoints.is_empty() {
                info!("Timer: Total execution time: {}", self.elapsed_str());
                return;
            }

            info!("Timer checkpoints:");
            let mut prev = Duration::from_secs(0);

            for (_idx, (label, duration)) in self.checkpoints.iter().enumerate() {
                let delta = *duration - prev;
                info!(
                    "  {}: {} (+{})",
                    label,
                    format_duration(*duration),
                    format_duration(delta)
                );
                prev = *duration;
            }

            let total = self.elapsed();
            let delta = total - prev;
            info!(
                "  Total: {} (+{})",
                format_duration(total),
                format_duration(delta)
            );
        }
    }
}

/// String manipulation utility functions
pub mod string_utils {
    // No imports needed here

    /// Truncate a string to a maximum length and add an ellipsis if needed
    pub fn truncate_string(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len - 3])
        }
    }

    /// Format a file size in a human-readable format
    pub fn format_file_size(size: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if size < KB {
            return format!("{} B", size);
        } else if size < MB {
            return format!("{:.2} KB", size as f64 / KB as f64);
        } else if size < GB {
            return format!("{:.2} MB", size as f64 / MB as f64);
        } else if size < TB {
            return format!("{:.2} GB", size as f64 / GB as f64);
        } else {
            return format!("{:.2} TB", size as f64 / TB as f64);
        }
    }

    /// Replace placeholders in a template string
    pub fn replace_placeholders(template: &str, replacements: &[(&str, &str)]) -> String {
        let mut result = template.to_string();
        for (key, value) in replacements {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }
}

/// HTTP utility functions
pub mod http_utils {
    use super::*;

    /// Extract a value from a cookie string
    pub fn extract_cookie_value(cookie_string: &str, name: &str) -> Option<String> {
        let cookie_regex = format!(r"{}=([^;]+)", regex::escape(name));
        match Regex::new(&cookie_regex) {
            Ok(re) => re
                .captures(cookie_string)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string())),
            Err(_) => None,
        }
    }

    /// Sanitize a URL for logging (remove sensitive parameters)
    pub fn sanitize_url(url: &str) -> String {
        // Define patterns for sensitive parameters
        let patterns = [
            (r"token=([^&]+)", "token=REDACTED"),
            (r"key=([^&]+)", "key=REDACTED"),
            (r"apikey=([^&]+)", "apikey=REDACTED"),
            (r"password=([^&]+)", "password=REDACTED"),
            (r"secret=([^&]+)", "secret=REDACTED"),
            (r"auth=([^&]+)", "auth=REDACTED"),
        ];

        let mut result = url.to_string();
        for (pattern, replacement) in patterns.iter() {
            if let Ok(re) = Regex::new(pattern) {
                result = re.replace_all(&result, *replacement).to_string();
            }
        }

        result
    }
}

/// Error handling utilities
pub mod error_utils {
    use super::*;

    /// Check if an error is retryable
    pub fn is_retryable_error(err: &anyhow::Error) -> bool {
        let err_string = err.to_string().to_lowercase();

        err_string.contains("timeout")
            || err_string.contains("connection")
            || err_string.contains("reset")
            || err_string.contains("temporary")
            || err_string.contains("retry")
            || err_string.contains("rate limit")
            || err_string.contains("too many requests")
    }

    /// Log an error with context
    pub fn log_error_with_context(err: &anyhow::Error, context: &str) {
        error!("{}: {}", context, err);

        let mut source = err.source();
        let mut depth = 0;

        while let Some(err) = source {
            if depth < 5 {
                // Limit recursion depth
                error!("Caused by: {}", err);
            }
            source = err.source();
            depth += 1;
        }
    }

    /// Create a specialized error based on error type
    pub fn categorize_error(err: &anyhow::Error) -> ErrorCategory {
        let err_string = err.to_string().to_lowercase();

        if err_string.contains("authentication") || err_string.contains("unauthorized") {
            ErrorCategory::Authentication
        } else if err_string.contains("permission") || err_string.contains("access denied") {
            ErrorCategory::Permission
        } else if err_string.contains("not found") || err_string.contains("404") {
            ErrorCategory::NotFound
        } else if err_string.contains("timeout") || err_string.contains("timed out") {
            ErrorCategory::Timeout
        } else if err_string.contains("rate limit") || err_string.contains("too many requests") {
            ErrorCategory::RateLimit
        } else if err_string.contains("quota") || err_string.contains("limit exceeded") {
            ErrorCategory::QuotaExceeded
        } else if err_string.contains("connection") || err_string.contains("network") {
            ErrorCategory::Network
        } else if err_string.contains("parse") || err_string.contains("syntax") {
            ErrorCategory::Parse
        } else if err_string.contains("io") || err_string.contains("file") {
            ErrorCategory::IO
        } else {
            ErrorCategory::Unknown
        }
    }

    /// Categories of errors for better handling
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorCategory {
        Authentication,
        Permission,
        NotFound,
        Timeout,
        RateLimit,
        QuotaExceeded,
        Network,
        Parse,
        IO,
        Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_apply() {
        let input = "test-123.mp4";
        let result = regex_utils::apply_regex(input, r"test-(\d+)\.mp4", "vid-$1.mp4").unwrap();
        assert_eq!(result, "vid-123.mp4");
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(string_utils::truncate_string("Hello", 10), "Hello");
        assert_eq!(string_utils::truncate_string("Hello World", 8), "Hello...");
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(string_utils::format_file_size(500), "500 B");
        assert_eq!(string_utils::format_file_size(1500), "1.46 KB");
        assert_eq!(string_utils::format_file_size(1500000), "1.43 MB");
    }

    #[test]
    fn test_extract_cookie_value() {
        let cookie = "session=abc123; user=john; token=xyz789";
        assert_eq!(
            http_utils::extract_cookie_value(cookie, "session"),
            Some("abc123".to_string())
        );
        assert_eq!(
            http_utils::extract_cookie_value(cookie, "user"),
            Some("john".to_string())
        );
        assert_eq!(
            http_utils::extract_cookie_value(cookie, "nonexistent"),
            None
        );
    }
}
