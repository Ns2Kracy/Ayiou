use std::sync::Arc;

use anyhow::Result;

use crate::onebot::{
    bot::Bot,
    model::{
        GroupMessageEvent, Message, MessageEvent, MessageSegment, OneBotEvent, PrivateMessageEvent,
    },
};

/// Message event type
#[derive(Clone)]
pub enum MsgEvent {
    Private(Arc<PrivateMessageEvent>),
    Group(Arc<GroupMessageEvent>),
}

/// Message context
#[derive(Clone)]
pub struct Ctx {
    bot: Bot,
    event: Arc<OneBotEvent>,
    msg: MsgEvent,
}

impl Ctx {
    /// Create context from OneBot event
    pub fn new(event: Arc<OneBotEvent>, bot: Bot) -> Option<Self> {
        let OneBotEvent::Message(msg_event) = event.as_ref() else {
            return None;
        };

        let msg = match msg_event.as_ref() {
            MessageEvent::Private(p) => MsgEvent::Private(Arc::new(p.clone())),
            MessageEvent::Group(g) => MsgEvent::Group(Arc::new(g.clone())),
        };

        Some(Self { bot, event, msg })
    }

    /// Get the underlying Bot for direct API calls
    #[inline]
    pub fn bot(&self) -> &Bot {
        &self.bot
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
                self.bot.send_private_msg(p.user_id, &msg).await?;
            }
            MsgEvent::Group(g) => {
                self.bot.send_group_msg(g.group_id, &msg).await?;
            }
        };
        Ok(())
    }

    /// Reply with text
    pub async fn reply_text(&self, text: impl Into<String>) -> Result<()> {
        self.reply(Message::String(text.into())).await
    }
}
