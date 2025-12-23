use std::sync::Arc;

use anyhow::Result;
use serde_json::json;
use tokio::sync::mpsc;

use crate::adapter::onebot::v11::model::{
    GroupMessageEvent, Message, MessageEvent, MessageSegment, OneBotEvent, PrivateMessageEvent,
};

/// Message event type
#[derive(Clone)]
pub enum MsgEvent {
    Private(Arc<PrivateMessageEvent>),
    Group(Arc<GroupMessageEvent>),
}

use crate::core::adapter::MsgContext;

/// Message context
#[derive(Clone)]
pub struct Ctx {
    event: Arc<OneBotEvent>,
    msg: MsgEvent,
    outgoing_tx: mpsc::Sender<String>,
}

impl MsgContext for Ctx {
    fn text(&self) -> String {
        self.text()
    }

    fn user_id(&self) -> String {
        self.user_id().to_string()
    }

    fn group_id(&self) -> Option<String> {
        self.group_id().map(|id| id.to_string())
    }
}

impl Ctx {
    /// Create context from OneBot event
    pub fn new(event: Arc<OneBotEvent>, outgoing_tx: mpsc::Sender<String>) -> Option<Self> {
        let OneBotEvent::Message(msg_event) = event.as_ref() else {
            return None;
        };

        let msg = match msg_event.as_ref() {
            MessageEvent::Private(p) => MsgEvent::Private(Arc::new(*p.clone())),
            MessageEvent::Group(g) => MsgEvent::Group(Arc::new(*g.clone())),
        };

        Some(Self {
            event,
            msg,
            outgoing_tx,
        })
    }

    /// Get raw OneBot event
    #[inline]
    pub fn event(&self) -> &OneBotEvent {
        &self.event
    }

    /// Get plain text from message
    pub fn text(&self) -> String {
        let message = match &self.msg {
            MsgEvent::Private(p) => &p.message,
            MsgEvent::Group(g) => &g.message,
        };

        match message {
            Message::String(s) => s.trim().to_string(),
            Message::Array(segments) => segments
                .iter()
                .filter_map(|seg| {
                    if let MessageSegment::Text { text } = seg {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string(),
        }
    }

    /// Raw message string
    #[inline]
    pub fn raw_message(&self) -> &str {
        match &self.msg {
            MsgEvent::Private(p) => &p.raw_message,
            MsgEvent::Group(g) => &g.raw_message,
        }
    }

    /// Sender user ID
    #[inline]
    pub fn user_id(&self) -> i64 {
        match &self.msg {
            MsgEvent::Private(p) => p.user_id,
            MsgEvent::Group(g) => g.user_id,
        }
    }

    /// Group ID (None for private messages)
    #[inline]
    pub fn group_id(&self) -> Option<i64> {
        match &self.msg {
            MsgEvent::Private(_) => None,
            MsgEvent::Group(g) => Some(g.group_id),
        }
    }

    /// Check if private message
    #[inline]
    pub fn is_private(&self) -> bool {
        matches!(self.msg, MsgEvent::Private(_))
    }

    /// Check if group message
    #[inline]
    pub fn is_group(&self) -> bool {
        matches!(self.msg, MsgEvent::Group(_))
    }

    /// Sender nickname
    #[inline]
    pub fn nickname(&self) -> &str {
        match &self.msg {
            MsgEvent::Private(p) => &p.sender.nickname,
            MsgEvent::Group(g) => &g.sender.nickname,
        }
    }

    /// Reply with message
    pub async fn reply(&self, message: impl Into<Message>) -> Result<()> {
        let msg = message.into();
        match &self.msg {
            MsgEvent::Private(p) => {
                self.send_private_msg(p.user_id, &msg).await?;
            }
            MsgEvent::Group(g) => {
                self.send_group_msg(g.group_id, &msg).await?;
            }
        };
        Ok(())
    }

    /// Reply with text
    pub async fn reply_text(&self, text: impl Into<String>) -> Result<()> {
        self.reply(Message::String(text.into())).await
    }
}

impl Ctx {
    /// Call API without waiting for response
    pub async fn call(&self, action: &str, params: serde_json::Value) -> Result<()> {
        let json = serde_json::to_string(&super::model::ApiRequest {
            action: action.to_string(),
            params,
            echo: None,
        })?;
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
