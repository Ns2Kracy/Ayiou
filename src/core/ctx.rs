use std::sync::Arc;

use anyhow::Result;

use crate::{
    core::message::MsgEvent,
    onebot::{
        api::Api,
        model::{Message, MessageEvent, MessageSegment, OneBotEvent},
    },
};

/// 消息上下文（所有字段都是 Arc，clone 零成本）
#[derive(Clone)]
pub struct Ctx {
    api: Api,
    event: Arc<OneBotEvent>,
    msg: MsgEvent,
}

impl Ctx {
    /// 从 OneBot 事件创建上下文
    pub(crate) fn new(event: Arc<OneBotEvent>, api: Api) -> Option<Self> {
        let OneBotEvent::Message(msg_event) = event.as_ref() else {
            return None;
        };

        let msg = match msg_event.as_ref() {
            MessageEvent::Private(p) => MsgEvent::Private(Arc::new(p.clone())),
            MessageEvent::Group(g) => MsgEvent::Group(Arc::new(g.clone())),
        };

        Some(Self { api, event, msg })
    }

    /// 获取 API 引用
    #[inline]
    pub fn api(&self) -> &Api {
        &self.api
    }

    /// 消息文本（提取纯文本内容）
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
    #[inline]
    pub fn raw_message(&self) -> &str {
        match &self.msg {
            MsgEvent::Private(p) => &p.raw_message,
            MsgEvent::Group(g) => &g.raw_message,
        }
    }

    /// 发送者 ID
    #[inline]
    pub fn user_id(&self) -> i64 {
        match &self.msg {
            MsgEvent::Private(p) => p.user_id,
            MsgEvent::Group(g) => g.user_id,
        }
    }

    /// 群 ID（私聊返回 None）
    #[inline]
    pub fn group_id(&self) -> Option<i64> {
        match &self.msg {
            MsgEvent::Private(_) => None,
            MsgEvent::Group(g) => Some(g.group_id),
        }
    }

    /// 是否私聊
    #[inline]
    pub fn is_private(&self) -> bool {
        matches!(self.msg, MsgEvent::Private(_))
    }

    /// 是否群聊
    #[inline]
    pub fn is_group(&self) -> bool {
        matches!(self.msg, MsgEvent::Group(_))
    }

    /// 发送者昵称
    #[inline]
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
    #[inline]
    pub fn event(&self) -> &OneBotEvent {
        &self.event
    }
}
