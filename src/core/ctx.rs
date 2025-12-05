use std::sync::Arc;

use anyhow::Result;

use crate::{
    core::message::MsgEvent,
    onebot::{
        api::Api,
        model::{Message, MessageEvent, MessageSegment, OneBotEvent},
    },
};

/// 消息上下文
#[derive(Clone)]
pub struct Ctx {
    pub api: Api,
    event: Arc<OneBotEvent>,
    msg: MsgEvent,
}

impl Ctx {
    pub(crate) fn from_event(event: Arc<OneBotEvent>, api: Api) -> Option<Self> {
        let OneBotEvent::Message(msg_event) = event.as_ref() else {
            return None;
        };

        let msg = match msg_event.as_ref() {
            MessageEvent::Private(p) => MsgEvent::Private(p.clone()),
            MessageEvent::Group(g) => MsgEvent::Group(g.clone()),
        };

        Some(Self { api, event, msg })
    }

    /// 消息文本
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

    /// 原始消息
    pub fn raw_message(&self) -> &str {
        match &self.msg {
            MsgEvent::Private(p) => &p.raw_message,
            MsgEvent::Group(g) => &g.raw_message,
        }
    }

    /// 发送者 ID
    pub fn user_id(&self) -> i64 {
        match &self.msg {
            MsgEvent::Private(p) => p.user_id,
            MsgEvent::Group(g) => g.user_id,
        }
    }

    /// 群 ID
    pub fn group_id(&self) -> Option<i64> {
        match &self.msg {
            MsgEvent::Private(_) => None,
            MsgEvent::Group(g) => Some(g.group_id),
        }
    }

    /// 是否私聊
    pub fn is_private(&self) -> bool {
        matches!(self.msg, MsgEvent::Private(_))
    }

    /// 是否群聊
    pub fn is_group(&self) -> bool {
        matches!(self.msg, MsgEvent::Group(_))
    }

    /// 发送者昵称
    pub fn nickname(&self) -> &str {
        match &self.msg {
            MsgEvent::Private(p) => &p.sender.nickname,
            MsgEvent::Group(g) => &g.sender.nickname,
        }
    }

    /// 回复消息
    pub async fn reply(&self, message: impl Into<Message>) -> Result<()> {
        let msg = message.into();
        match &self.msg {
            MsgEvent::Private(p) => self.api.send_private_msg(p.user_id, &msg).await?,
            MsgEvent::Group(g) => self.api.send_group_msg(g.group_id, &msg).await?,
        };
        Ok(())
    }

    /// 回复文本
    pub async fn reply_text(&self, text: impl Into<String>) -> Result<()> {
        self.reply(Message::String(text.into())).await
    }

    /// 原始事件
    pub fn event(&self) -> &OneBotEvent {
        &self.event
    }
}
