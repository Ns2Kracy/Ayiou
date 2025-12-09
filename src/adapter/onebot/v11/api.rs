use anyhow::Result;
use serde_json::json;
use tokio::sync::mpsc;

use crate::adapter::onebot::v11::{adapter::OneBotV11Adapter, model::Message};

/// OneBot bot instance
///
/// Represents a bot connected to OneBot service, provides API call capabilities
#[derive(Clone)]
pub struct Api {
    outgoing_tx: mpsc::Sender<String>,
}

impl Api {
    pub fn new(outgoing_tx: mpsc::Sender<String>) -> Self {
        Self { outgoing_tx }
    }

    /// Call API without waiting for response
    pub async fn call(&self, action: &str, params: serde_json::Value) -> Result<()> {
        let json = OneBotV11Adapter::build_request(action, params)?;
        self.outgoing_tx.send(json).await?;
        Ok(())
    }

    /// Send private message
    pub async fn send_private_msg(&self, user_id: i64, message: &Message) -> Result<()> {
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
    pub async fn send_group_msg(&self, group_id: i64, message: &Message) -> Result<()> {
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
    pub async fn kick_group_member(&self, group_id: i64, user_id: i64) -> Result<()> {
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
    pub async fn delete_msg(&self, message_id: i32) -> Result<()> {
        self.call("delete_msg", json!({ "message_id": message_id }))
            .await
    }

    /// Get login info
    pub async fn get_login_info(&self) -> Result<()> {
        self.call("get_login_info", json!({})).await
    }

    /// Get group info
    pub async fn get_group_info(&self, group_id: i64) -> Result<()> {
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
    pub async fn get_group_member_info(&self, group_id: i64, user_id: i64) -> Result<()> {
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
    pub async fn set_group_ban(&self, group_id: i64, user_id: i64, duration: i64) -> Result<()> {
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
