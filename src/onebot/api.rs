use anyhow::Result;
use serde_json::json;
use tokio::sync::mpsc;

use crate::onebot::model::{ApiRequest, Message};

/// 一个具体的 Bot 连接实例的 API 客户端
#[derive(Clone)]
pub struct Api {
    tx: mpsc::Sender<String>,
}

impl Api {
    pub fn new(tx: mpsc::Sender<String>) -> Self {
        Self { tx }
    }

    async fn call_api(&self, action: &str, params: serde_json::Value) -> Result<()> {
        let req = ApiRequest {
            action: action.to_string(),
            params,
            echo: None,
        };
        let json_req = serde_json::to_string(&req)?;
        self.tx.send(json_req).await?;
        Ok(())
    }

    pub async fn send_private_msg(&self, user_id: i64, message: &Message) -> Result<()> {
        self.call_api(
            "send_private_msg",
            json!({
                "user_id": user_id,
                "message": message,
            }),
        )
        .await
    }

    pub async fn send_group_msg(&self, group_id: i64, message: &Message) -> Result<()> {
        self.call_api(
            "send_group_msg",
            json!({
                "group_id": group_id,
                "message": message,
            }),
        )
        .await
    }

    pub async fn kick_group_member(&self, group_id: i64, user_id: i64) -> Result<()> {
        self.call_api(
            "set_group_kick",
            json!({
                "group_id": group_id,
                "user_id": user_id,
                "reject_add_request": false
            }),
        )
        .await
    }

    // You can add more strongly-typed API methods here
}
