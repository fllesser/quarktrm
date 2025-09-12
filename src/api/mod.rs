//! API client for Quark Drive
//!
//! This module contains the API client for interacting with Quark Drive's APIs.
//! It handles authentication, file listing, sharing, and file transfers.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use log::{error, info, warn};
use reqwest::{
    Client, ClientBuilder, StatusCode,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::models::{
    ApiError, FileInfo, FileListResponse, FileTransferResponse, ShareInfo, ShareStatus, User,
};

/// API endpoints for Quark Drive
const API_BASE_URL: &str = "https://drive.quark.cn/1/clouddrive";
//const API_LOGIN_URL: &str = "https://drive.quark.cn/1/auth/login";
const API_USER_INFO_URL: &str = "https://drive.quark.cn/1/clouddrive/user/info";
const API_FILE_LIST_URL: &str = "https://drive.quark.cn/1/clouddrive/file/list";
const API_SHARE_INFO_URL: &str = "https://drive.quark.cn/1/clouddrive/share/info";
//const API_SHARE_LIST_URL: &str = "https://drive.quark.cn/1/clouddrive/share/list";
//const API_TRANSFER_URL: &str = "https://drive.quark.cn/1/clouddrive/file/transfer";
const API_SIGN_IN_URL: &str = "https://drive.quark.cn/1/clouddrive/sign_in";

/// Default user agent if none is provided
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36";

/// Default timeout for API requests in seconds
const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

/// Maximum number of retry attempts for API requests
const MAX_RETRY_ATTEMPTS: usize = 3;

/// Client for interacting with Quark Drive API
pub struct QuarkClient {
    /// HTTP client for making requests
    client: Client,

    /// Authentication cookies
    cookie: Arc<Mutex<String>>,

    /// Authentication token
    token: Arc<Mutex<Option<String>>>,

    /// User information
    user_info: Arc<Mutex<Option<User>>>,
}

impl QuarkClient {
    /// Create a new Quark Drive API client
    pub fn new(cookie: String, token: Option<String>, user_agent: Option<String>) -> Result<Self> {
        let mut headers = HeaderMap::new();

        // Set default headers
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("application/json"),
        );

        let ua = user_agent.unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_str(&ua).context("Invalid user agent string")?,
        );

        let client = ClientBuilder::new()
            .default_headers(headers)
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECONDS))
            .cookie_store(true)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            cookie: Arc::new(Mutex::new(cookie)),
            token: Arc::new(Mutex::new(token)),
            user_info: Arc::new(Mutex::new(None)),
        })
    }

    /// Verify authentication by fetching user info
    pub async fn verify_auth(&self) -> Result<User> {
        let user = self.get_user_info().await?;
        info!("Authentication verified for user ID: {}", user.user_id);
        Ok(user)
    }

    /// Get user information
    pub async fn get_user_info(&self) -> Result<User> {
        // Check if we already have user info cached
        {
            let user_info = self.user_info.lock().await;
            if let Some(user) = &*user_info {
                return Ok(user.clone());
            }
        }

        // Fetch user info
        let response = self
            .authenticated_request::<serde_json::Value>(
                API_USER_INFO_URL,
                reqwest::Method::GET,
                None,
            )
            .await?;

        #[derive(Deserialize)]
        struct UserInfoResponse {
            data: User,
        }

        let user_response: UserInfoResponse = response
            .json()
            .await
            .context("Failed to parse user info response")?;

        // Cache the user info
        {
            let mut user_info = self.user_info.lock().await;
            *user_info = Some(user_response.data.clone());
        }

        Ok(user_response.data)
    }

    /// Perform daily sign-in to earn storage space
    pub async fn daily_sign_in(&self) -> Result<bool> {
        let response = self
            .authenticated_request::<serde_json::Value>(
                API_SIGN_IN_URL,
                reqwest::Method::POST,
                None,
            )
            .await?;

        if response.status().is_success() {
            info!("Daily sign-in successful");
            Ok(true)
        } else {
            warn!("Daily sign-in failed with status: {}", response.status());
            Ok(false)
        }
    }

    /// Get information about a shared link
    pub async fn get_share_info(
        &self,
        share_url: &str,
        extraction_code: Option<&str>,
    ) -> Result<ShareInfo> {
        let share_id = self
            .extract_share_id(share_url)
            .ok_or_else(|| anyhow!("Invalid share URL: {}", share_url))?;

        let mut params = HashMap::new();
        params.insert("share_id", share_id);

        if let Some(code) = extraction_code {
            params.insert("extraction_code", code.to_string());
        }

        let response = self
            .authenticated_request::<HashMap<&str, String>>(
                API_SHARE_INFO_URL,
                reqwest::Method::GET,
                Some(params),
            )
            .await?;

        #[derive(Deserialize)]
        struct ShareInfoResponse {
            data: ShareInfo,
        }

        let share_response: ShareInfoResponse = response
            .json()
            .await
            .context("Failed to parse share info response")?;

        Ok(share_response.data)
    }

    /// Check if a share is valid
    pub async fn check_share_status(
        &self,
        share_url: &str,
        extraction_code: Option<&str>,
    ) -> Result<ShareStatus> {
        match self.get_share_info(share_url, extraction_code).await {
            Ok(info) => {
                if !info.is_valid {
                    return Ok(ShareStatus::Invalid);
                }

                if info.requires_extraction_code && extraction_code.is_none() {
                    return Ok(ShareStatus::RequiresExtractionCode);
                }

                if let Some(expires_at) = info.expires_at {
                    if expires_at < chrono::Utc::now() {
                        return Ok(ShareStatus::Expired);
                    }
                }

                Ok(ShareStatus::Valid)
            }
            Err(err) => {
                error!("Failed to check share status: {}", err);
                // Try to determine if the error is due to extraction code requirement
                if err.to_string().contains("extraction_code") {
                    Ok(ShareStatus::RequiresExtractionCode)
                } else {
                    Ok(ShareStatus::Invalid)
                }
            }
        }
    }

    /// List files in a shared folder
    pub async fn list_shared_files(
        &self,
        share_url: &str,
        parent_id: Option<&str>,
        extraction_code: Option<&str>,
        marker: Option<&str>,
        limit: Option<u32>,
    ) -> Result<FileListResponse> {
        let share_id = self
            .extract_share_id(share_url)
            .ok_or_else(|| anyhow!("Invalid share URL: {}", share_url))?;

        let mut params = HashMap::new();
        params.insert("share_id", share_id);

        if let Some(id) = parent_id {
            params.insert("parent_id", id.to_string());
        }

        if let Some(code) = extraction_code {
            params.insert("extraction_code", code.to_string());
        }

        if let Some(m) = marker {
            params.insert("marker", m.to_string());
        }

        if let Some(l) = limit {
            params.insert("limit", l.to_string());
        } else {
            params.insert("limit", "100".to_string());
        }

        let response = self
            .authenticated_request::<HashMap<&str, String>>(
                &format!("{}/share/file/list", API_BASE_URL),
                reqwest::Method::GET,
                Some(params),
            )
            .await?;

        #[derive(Deserialize)]
        struct FileListWrapper {
            data: FileListResponse,
        }

        let file_list: FileListWrapper = response
            .json()
            .await
            .context("Failed to parse file list response")?;

        Ok(file_list.data)
    }

    /// List files in the user's own drive
    pub async fn list_files(
        &self,
        parent_id: Option<&str>,
        marker: Option<&str>,
        limit: Option<u32>,
    ) -> Result<FileListResponse> {
        let mut params = HashMap::new();

        if let Some(id) = parent_id {
            params.insert("parent_id", id.to_string());
        } else {
            params.insert("parent_id", "root".to_string());
        }

        if let Some(m) = marker {
            params.insert("marker", m.to_string());
        }

        if let Some(l) = limit {
            params.insert("limit", l.to_string());
        } else {
            params.insert("limit", "100".to_string());
        }

        let response = self
            .authenticated_request::<HashMap<&str, String>>(
                API_FILE_LIST_URL,
                reqwest::Method::GET,
                Some(params),
            )
            .await?;

        #[derive(Deserialize)]
        struct FileListWrapper {
            data: FileListResponse,
        }

        let file_list: FileListWrapper = response
            .json()
            .await
            .context("Failed to parse file list response")?;

        Ok(file_list.data)
    }

    /// Create a folder in the user's drive
    pub async fn create_folder(&self, parent_id: &str, folder_name: &str) -> Result<FileInfo> {
        let payload = json!({
            "parent_id": parent_id,
            "name": folder_name,
            "type": "folder"
        });

        let response = self
            .authenticated_request::<serde_json::Value>(
                &format!("{}/file/create", API_BASE_URL),
                reqwest::Method::POST,
                Some(payload),
            )
            .await?;

        #[derive(Deserialize)]
        struct FolderCreateResponse {
            data: FileInfo,
        }

        let folder_response: FolderCreateResponse = response
            .json()
            .await
            .context("Failed to parse folder create response")?;

        Ok(folder_response.data)
    }

    /// Find or create a folder path
    pub async fn find_or_create_folder_path(&self, path: &str) -> Result<String> {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Ok("root".to_string());
        }

        let mut current_id = "root".to_string();

        for part in parts {
            // Check if folder exists
            let files = self.list_files(Some(&current_id), None, None).await?;
            let found = files
                .items
                .iter()
                .find(|f| f.name == part && f.is_directory);

            current_id = match found {
                Some(folder) => folder.file_id.clone(),
                None => {
                    // Create the folder
                    let new_folder = self.create_folder(&current_id, part).await?;
                    new_folder.file_id
                }
            };
        }

        Ok(current_id)
    }

    /// Transfer a file from a share to the user's drive
    pub async fn transfer_file_from_share(
        &self,
        share_url: &str,
        file_id: &str,
        target_folder_id: &str,
        new_name: Option<&str>,
        extraction_code: Option<&str>,
    ) -> Result<FileTransferResponse> {
        let share_id = self
            .extract_share_id(share_url)
            .ok_or_else(|| anyhow!("Invalid share URL: {}", share_url))?;

        let mut payload = json!({
            "share_id": share_id,
            "file_id": file_id,
            "target_folder_id": target_folder_id
        });

        if let Some(name) = new_name {
            payload["new_name"] = json!(name);
        }

        if let Some(code) = extraction_code {
            payload["extraction_code"] = json!(code);
        }

        let response = self
            .authenticated_request::<serde_json::Value>(
                &format!("{}/file/transfer_from_share", API_BASE_URL),
                reqwest::Method::POST,
                Some(payload),
            )
            .await?;

        #[derive(Deserialize)]
        struct TransferResponse {
            data: FileTransferResponse,
        }

        let transfer: TransferResponse = response
            .json()
            .await
            .context("Failed to parse file transfer response")?;

        Ok(transfer.data)
    }

    /// Extract the share ID from a share URL
    fn extract_share_id(&self, share_url: &str) -> Option<String> {
        // Extract share ID from URL like https://pan.quark.cn/s/xxxxxxxx
        if let Some(idx) = share_url.find("/s/") {
            let id = &share_url[idx + 3..];
            let id = id.split('/').next().unwrap_or(id);
            let id = id.split('?').next().unwrap_or(id);
            return Some(id.to_string());
        }
        None
    }

    /// Make an authenticated request to the Quark Drive API
    async fn authenticated_request<T>(
        &self,
        url: &str,
        method: reqwest::Method,
        data: Option<T>,
    ) -> Result<reqwest::Response>
    where
        T: Serialize + Send + Sync + 'static,
    {
        // Prepare the request with authentication
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < MAX_RETRY_ATTEMPTS {
            let mut req = self.client.request(method.clone(), url);

            // Add cookie header
            {
                let cookie = self.cookie.lock().await;
                req = req.header("Cookie", &*cookie);
            }

            // Add authorization token if available
            {
                let token = self.token.lock().await;
                if let Some(t) = &*token {
                    req = req.header("Authorization", format!("Bearer {}", t));
                }
            }

            // Add request data
            if let Some(d) = &data {
                if method == reqwest::Method::GET {
                    // For GET requests, convert the data to query parameters
                    match serde_json::to_value(d) {
                        Ok(map) => {
                            if let Some(obj) = map.as_object() {
                                for (k, v) in obj {
                                    if let Some(value_str) = v.as_str() {
                                        req = req.query(&[(k, value_str)]);
                                    } else {
                                        req = req.query(&[(k, v.to_string())]);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to convert data to JSON: {}", e);
                        }
                    }
                } else {
                    // For other methods, add as JSON body
                    req = req.json(d);
                }
            }

            // Send the request
            match req.send().await {
                Ok(response) => {
                    // Check for authentication errors
                    if response.status() == StatusCode::UNAUTHORIZED {
                        error!("Authentication failed: {}", response.status());

                        // Try to refresh the token here if needed
                        attempts += 1;
                        continue;
                    }

                    // Check for other error responses
                    if !response.status().is_success() {
                        let status = response.status();
                        let error_text = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Failed to read error response".to_string());

                        error!("API request failed: {} - {}", status, error_text);

                        // Deserialize the error if possible
                        if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_text) {
                            last_error = Some(anyhow!(
                                "API error: {} - {}",
                                api_error.code,
                                api_error.message
                            ));
                        } else {
                            last_error = Some(anyhow!("API error: {} - {}", status, error_text));
                        }

                        attempts += 1;
                        continue;
                    }

                    // Return successful response
                    return Ok(response);
                }
                Err(err) => {
                    error!("Request error: {}", err);
                    last_error = Some(err.into());
                    attempts += 1;

                    // Add a small delay before retrying
                    tokio::time::sleep(Duration::from_millis(500 * attempts as u64)).await;
                }
            }
        }

        // If we reached here, all attempts failed
        Err(last_error
            .unwrap_or_else(|| anyhow!("API request failed after {} attempts", MAX_RETRY_ATTEMPTS)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_extract_share_id() {
        let client = QuarkClient::new("test_cookie".to_string(), None, None).unwrap();

        assert_eq!(
            client.extract_share_id("https://pan.quark.cn/s/abc123"),
            Some("abc123".to_string())
        );

        assert_eq!(
            client.extract_share_id("https://pan.quark.cn/s/abc123/"),
            Some("abc123".to_string())
        );

        assert_eq!(
            client.extract_share_id("https://pan.quark.cn/s/abc123?pwd=test"),
            Some("abc123".to_string())
        );

        assert_eq!(client.extract_share_id("not a valid url"), None);
    }
}
