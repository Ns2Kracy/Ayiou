use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde_json::json;
use tokio::sync::mpsc;

use crate::core::BotAdapter;
use crate::onebot::adapter::{ApiResponse, OneBotAdapter};
use crate::onebot::model::Message;

/// OneBot bot instance
///
/// Represents a bot connected to OneBot service, provides API call capabilities
#[derive(Clone)]
pub struct Bot {
    adapter: Arc<OneBotAdapter>,
    outgoing_tx: mpsc::Sender<String>,
}

impl Bot {
    pub fn new(adapter: Arc<OneBotAdapter>, outgoing_tx: mpsc::Sender<String>) -> Self {
        Self {
            adapter,
            outgoing_tx,
        }
    }

    /// Call API and wait for response
    pub async fn call(&self, action: &str, params: serde_json::Value) -> Result<ApiResponse> {
        let (json, rx) = self.adapter.build_request_with_echo(action, params).await?;
        self.outgoing_tx.send(json).await?;

        match tokio::time::timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => {
                if response.is_ok() {
                    Ok(response)
                } else {
                    Err(anyhow!(
                        "API error: status={}, retcode={}",
                        response.status,
                        response.retcode
                    ))
                }
            }
            Ok(Err(_)) => Err(anyhow!("Response channel closed")),
            Err(_) => Err(anyhow!("API call timeout")),
        }
    }

    /// Call API without waiting for response
    pub async fn call_no_wait(&self, action: &str, params: serde_json::Value) -> Result<()> {
        let json = self.adapter.build_request(action, params)?;
        self.outgoing_tx.send(json).await?;
        Ok(())
    }

    /// Send private message
    pub async fn send_private_msg(&self, user_id: i64, message: &Message) -> Result<ApiResponse> {
        self.call(
            "send_private_msg",
            json!({
                "user_id": user_id,
                "message": message,
            }),
        )
        .await
    }

    /// Send group message
    pub async fn send_group_msg(&self, group_id: i64, message: &Message) -> Result<ApiResponse> {
        self.call(
            "send_group_msg",
            json!({
                "group_id": group_id,
                "message": message,
            }),
        )
        .await
    }

    /// Kick group member
    pub async fn kick_group_member(&self, group_id: i64, user_id: i64) -> Result<ApiResponse> {
        self.call(
            "set_group_kick",
            json!({
                "group_id": group_id,
                "user_id": user_id,
                "reject_add_request": false
            }),
        )
        .await
    }

    /// Delete/recall message
    pub async fn delete_msg(&self, message_id: i32) -> Result<ApiResponse> {
        self.call("delete_msg", json!({ "message_id": message_id }))
            .await
    }

    /// Get login info
    pub async fn get_login_info(&self) -> Result<ApiResponse> {
        self.call("get_login_info", json!({})).await
    }

    /// Get group info
    pub async fn get_group_info(&self, group_id: i64) -> Result<ApiResponse> {
        self.call(
            "get_group_info",
            json!({
                "group_id": group_id,
                "no_cache": false
            }),
        )
        .await
    }

    /// Get group member info
    pub async fn get_group_member_info(&self, group_id: i64, user_id: i64) -> Result<ApiResponse> {
        self.call(
            "get_group_member_info",
            json!({
                "group_id": group_id,
                "user_id": user_id,
                "no_cache": false
            }),
        )
        .await
    }

    /// Set group ban
    pub async fn set_group_ban(
        &self,
        group_id: i64,
        user_id: i64,
        duration: i64,
    ) -> Result<ApiResponse> {
        self.call(
            "set_group_ban",
            json!({
                "group_id": group_id,
                "user_id": user_id,
                "duration": duration
            }),
        )
        .await
    }
}

impl BotAdapter for Bot {
    fn adapter_name(&self) -> &'static str {
        "onebot"
    }
}
