use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::adapter::onebot::v11::model::{Message, OneBotAction};
use crate::core::plugin_host::MessageSender;

#[derive(Clone)]
pub struct OneBotSender {
    outgoing_tx: mpsc::Sender<String>,
}

impl OneBotSender {
    pub fn new(outgoing_tx: mpsc::Sender<String>) -> Self {
        Self { outgoing_tx }
    }

    pub fn test_pair() -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(8);
        (Self::new(tx), rx)
    }

    async fn send_action(&self, action: OneBotAction) -> Result<()> {
        let request = action.into_request();
        let raw = serde_json::to_string(&request)?;
        self.outgoing_tx.send(raw).await?;
        Ok(())
    }
}

#[async_trait]
impl MessageSender for OneBotSender {
    async fn send_private_text(&self, user_id: i64, text: &str) -> Result<()> {
        self.send_action(OneBotAction::SendPrivateMsg {
            user_id,
            message: Message::String(text.to_string()),
        })
        .await
    }

    async fn send_group_text(&self, group_id: i64, text: &str) -> Result<()> {
        self.send_action(OneBotAction::SendGroupMsg {
            group_id,
            message: Message::String(text.to_string()),
        })
        .await
    }
}
