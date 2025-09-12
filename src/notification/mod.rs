//! Notification module for Quark Drive Auto-Save
//!
//! This module handles sending notifications about task results,
//! errors, and other important events to various notification services.
#![allow(unused)]

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use log::{debug, error, info};
use reqwest::Client;

use serde_json::json;

use crate::config::{Config, DiscordConfig, PushoverConfig, TelegramConfig, WebhookConfig};
use crate::models::{AppEvent, EventType};

/// Manages sending notifications through various channels
pub struct NotificationManager {
    /// HTTP client for making API requests
    client: Client,

    /// Notification configuration
    config: Arc<Config>,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to create HTTP client for notifications")?;

        Ok(Self { client, config })
    }

    /// Send a notification to all configured channels
    pub async fn send_notification(
        &self,
        title: &str,
        message: &str,
        task_id: Option<&str>,
    ) -> bool {
        // Check if notifications are globally enabled
        if !self.config.notification.enabled {
            debug!("Notifications are disabled, skipping: {}", title);
            return false;
        }

        let mut success = true;

        // Create an event for this notification
        let event = AppEvent {
            timestamp: Utc::now(),
            event_type: EventType::Info,
            message: message.to_string(),
            task_id: task_id.map(String::from),
            data: None,
        };

        // Send to Pushover if configured
        if let Some(pushover) = &self.config.notification.pushover {
            match self
                .send_pushover_notification(pushover, title, message)
                .await
            {
                Ok(_) => info!("Sent Pushover notification: {}", title),
                Err(err) => {
                    error!("Failed to send Pushover notification: {}", err);
                    success = false;
                }
            }
        }

        // Send to Telegram if configured
        if let Some(telegram) = &self.config.notification.telegram {
            match self
                .send_telegram_notification(telegram, title, message)
                .await
            {
                Ok(_) => info!("Sent Telegram notification: {}", title),
                Err(err) => {
                    error!("Failed to send Telegram notification: {}", err);
                    success = false;
                }
            }
        }

        // Send to Discord if configured
        if let Some(discord) = &self.config.notification.discord {
            match self
                .send_discord_notification(discord, title, message)
                .await
            {
                Ok(_) => info!("Sent Discord notification: {}", title),
                Err(err) => {
                    error!("Failed to send Discord notification: {}", err);
                    success = false;
                }
            }
        }

        // Send to custom webhook if configured
        if let Some(webhook) = &self.config.notification.webhook {
            match self
                .send_webhook_notification(webhook, title, message, &event)
                .await
            {
                Ok(_) => info!("Sent webhook notification: {}", title),
                Err(err) => {
                    error!("Failed to send webhook notification: {}", err);
                    success = false;
                }
            }
        }

        success
    }

    /// Send a notification through Pushover
    async fn send_pushover_notification(
        &self,
        config: &PushoverConfig,
        title: &str,
        message: &str,
    ) -> Result<()> {
        let params = [
            ("token", config.api_token.as_str()),
            ("user", config.user_key.as_str()),
            ("title", title),
            ("message", message),
        ];

        let response = self
            .client
            .post("https://api.pushover.net/1/messages.json")
            .form(&params)
            .send()
            .await
            .context("Failed to send Pushover request")?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("Pushover API error: {}", error_text));
        }

        Ok(())
    }

    /// Send a notification through Telegram
    async fn send_telegram_notification(
        &self,
        config: &TelegramConfig,
        title: &str,
        message: &str,
    ) -> Result<()> {
        let full_message = format!("*{}*\n{}", title, message);
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            config.bot_token
        );

        let params = json!({
            "chat_id": config.chat_id,
            "text": full_message,
            "parse_mode": "Markdown"
        });

        let response = self
            .client
            .post(url)
            .json(&params)
            .send()
            .await
            .context("Failed to send Telegram request")?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("Telegram API error: {}", error_text));
        }

        Ok(())
    }

    /// Send a notification through Discord webhook
    async fn send_discord_notification(
        &self,
        config: &DiscordConfig,
        title: &str,
        message: &str,
    ) -> Result<()> {
        let payload = json!({
            "embeds": [{
                "title": title,
                "description": message,
                "color": 3447003,
                "timestamp": Utc::now().to_rfc3339()
            }]
        });

        let response = self
            .client
            .post(&config.webhook_url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send Discord webhook request")?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("Discord webhook error: {}", error_text));
        }

        Ok(())
    }

    /// Send a notification through a custom webhook
    async fn send_webhook_notification(
        &self,
        config: &WebhookConfig,
        title: &str,
        message: &str,
        event: &AppEvent,
    ) -> Result<()> {
        // Determine the HTTP method to use
        let method = match config.method.to_uppercase().as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            _ => reqwest::Method::POST,
        };

        // Build the request
        let mut request_builder = self.client.request(method, &config.url);

        // Add headers if configured
        for (key, value) in &config.headers {
            request_builder = request_builder.header(key, value);
        }

        // Add body or query parameters
        if let Some(template) = &config.body_template {
            // Replace placeholders in the template
            let body = template
                .replace("{title}", title)
                .replace("{message}", message)
                .replace("{timestamp}", &event.timestamp.to_rfc3339());

            // Add body
            request_builder = request_builder.body(body);
        } else {
            // Default JSON payload
            let payload = json!({
                "title": title,
                "message": message,
                "timestamp": event.timestamp,
                "event_type": format!("{:?}", event.event_type),
                "task_id": event.task_id
            });

            request_builder = request_builder.json(&payload);
        }

        // Send the request
        let response = request_builder
            .send()
            .await
            .context("Failed to send webhook request")?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(anyhow::anyhow!("Webhook error: {}", error_text));
        }

        Ok(())
    }

    /// Send a notification about a task event
    pub async fn notify_task_event(
        &self,
        task_id: &str,
        task_name: &str,
        event_type: EventType,
        message: &str,
    ) -> bool {
        let title = match event_type {
            EventType::TaskStarted => format!("Task Started: {}", task_name),
            EventType::TaskCompleted => format!("Task Completed: {}", task_name),
            EventType::TaskFailed => format!("Task Failed: {}", task_name),
            _ => format!("Task Update: {}", task_name),
        };

        self.send_notification(&title, message, Some(task_id)).await
    }

    /// Send an error notification
    pub async fn notify_error(&self, title: &str, error: &str, task_id: Option<&str>) -> bool {
        let title = format!("Error: {}", title);
        self.send_notification(&title, error, task_id).await
    }
}

// #[cfg(test)]
// mod tests {
//     use std::collections::HashMap;

//     use super::*;
//     use mockito::Server;

//     fn setup_mock_server() -> Server {
//         let server = Server::new();
//     }

//     #[tokio::test]
//     async fn test_webhook_notification() {
//         let mock_server = mock("POST", "/webhook")
//             .with_status(200)
//             .with_header("content-type", "application/json")
//             .with_body("{\"success\":true}")
//             .create();

//         let webhook_config = WebhookConfig {
//             url: format!("{}/webhook", server_url()),
//             method: "POST".to_string(),
//             headers: HashMap::new(),
//             body_template: None,
//         };

//         let notification_config = crate::config::NotificationConfig {
//             enabled: true,
//             pushover: None,
//             telegram: None,
//             discord: None,
//             webhook: Some(webhook_config),
//         };

//         let config = Config {
//             auth: crate::config::AuthConfig {
//                 cookie: "test".to_string(),
//                 access_token: None,
//                 user_agent: None,
//             },
//             tasks: Vec::new(),
//             notification: notification_config,
//             media_library: None,
//             options: crate::config::GlobalOptions::default(),
//         };

//         let manager = NotificationManager::new(Arc::new(config)).unwrap();
//         let event = AppEvent {
//             timestamp: Utc::now(),
//             event_type: EventType::Info,
//             message: "Test message".to_string(),
//             task_id: None,
//             data: None,
//         };

//         let result = manager
//             .send_webhook_notification(
//                 manager.config.notification.webhook.as_ref().unwrap(),
//                 "Test Title",
//                 "Test Message",
//                 &event,
//             )
//             .await;

//         assert!(result.is_ok());
//         mock_server.assert();
//     }
// }
