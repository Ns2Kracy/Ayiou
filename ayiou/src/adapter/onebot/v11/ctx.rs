use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};

use crate::adapter::onebot::v11::model::{
    ApiRequest, ApiResponse, GroupInfoData, GroupMemberInfoData, LoginInfoData, Message,
    MessageEvent, MessageSegment, OneBotAction, OneBotEvent, SendMessageData, echo_key,
};
use crate::core::{
    context::Context,
    model::{
        BotId, ChannelRef, EventEnvelope, MessageEvent as KernelMessageEvent,
        MessageSegment as KernelMessageSegment, PlatformId, UserRef,
    },
    plugin::OutboundSender,
};

const API_TIMEOUT: Duration = Duration::from_secs(10);

/// Message context
#[derive(Clone)]
pub struct Ctx {
    event: Arc<OneBotEvent>,
    array_text: Option<String>,
    outgoing_tx: mpsc::Sender<String>,
    pending_api: Arc<DashMap<String, oneshot::Sender<ApiResponse>>>,
    echo_seq: Arc<AtomicU64>,
}

impl Ctx {
    /// Create context from `OneBot` event
    pub fn new(
        event: Arc<OneBotEvent>,
        outgoing_tx: mpsc::Sender<String>,
        pending_api: Arc<DashMap<String, oneshot::Sender<ApiResponse>>>,
        echo_seq: Arc<AtomicU64>,
    ) -> Option<Self> {
        let OneBotEvent::Message(msg_event) = event.as_ref() else {
            return None;
        };
        let message = message_payload(msg_event);
        let array_text = if let Message::Array(segments) = message {
            let mut text = String::new();
            for segment in segments {
                if let MessageSegment::Text { text: value } = segment {
                    text.push_str(value);
                }
            }
            let trimmed = text.trim();
            Some(if trimmed.len() == text.len() {
                text
            } else {
                trimmed.to_string()
            })
        } else {
            None
        };

        Some(Self {
            event,
            array_text,
            outgoing_tx,
            pending_api,
            echo_seq,
        })
    }

    /// Get raw `OneBot` event
    #[inline]
    #[must_use]
    pub fn event(&self) -> &OneBotEvent {
        &self.event
    }

    fn message_event(&self) -> &MessageEvent {
        match self.event.as_ref() {
            OneBotEvent::Message(message) => message,
            _ => unreachable!("OneBot Ctx only stores message events"),
        }
    }

    fn message(&self) -> &Message {
        message_payload(self.message_event())
    }

    /// Get plain text from message
    #[must_use]
    pub fn text(&self) -> Cow<'_, str> {
        match self.message() {
            Message::String(s) => Cow::Borrowed(s.trim()),
            Message::Segment(seg) => match seg {
                MessageSegment::Text { text } => Cow::Borrowed(text.trim()),
                _ => Cow::Borrowed(""),
            },
            Message::Array(_) => Cow::Borrowed(self.array_text.as_deref().unwrap_or("")),
        }
    }

    /// Raw message string
    #[inline]
    #[must_use]
    pub fn raw_message(&self) -> &str {
        match self.message_event() {
            MessageEvent::Private(p) => &p.raw_message,
            MessageEvent::Group(g) => &g.raw_message,
        }
    }

    /// Sender user ID
    #[inline]
    #[must_use]
    pub fn user_id(&self) -> i64 {
        match self.message_event() {
            MessageEvent::Private(p) => p.user_id,
            MessageEvent::Group(g) => g.user_id,
        }
    }

    /// Group ID (None for private messages)
    #[inline]
    #[must_use]
    pub fn group_id(&self) -> Option<i64> {
        match self.message_event() {
            MessageEvent::Private(_) => None,
            MessageEvent::Group(g) => Some(g.group_id),
        }
    }

    /// Check if private message
    #[inline]
    #[must_use]
    pub fn is_private(&self) -> bool {
        matches!(self.message_event(), MessageEvent::Private(_))
    }

    /// Check if group message
    #[inline]
    #[must_use]
    pub fn is_group(&self) -> bool {
        matches!(self.message_event(), MessageEvent::Group(_))
    }

    /// Sender nickname
    #[inline]
    #[must_use]
    pub fn nickname(&self) -> &str {
        match self.message_event() {
            MessageEvent::Private(p) => &p.sender.nickname,
            MessageEvent::Group(g) => &g.sender.nickname,
        }
    }

    /// Reply with message
    pub async fn reply(&self, message: impl Into<Message>) -> Result<()> {
        let msg = message.into();
        match self.message_event() {
            MessageEvent::Private(p) => {
                self.send_private_msg(p.user_id, &msg).await?;
            }
            MessageEvent::Group(g) => {
                self.send_group_msg(g.group_id, &msg).await?;
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn self_id(&self) -> i64 {
        match self.message_event() {
            MessageEvent::Private(p) => p.self_id,
            MessageEvent::Group(g) => g.self_id,
        }
    }

    #[must_use]
    pub fn outgoing_tx(&self) -> mpsc::Sender<String> {
        self.outgoing_tx.clone()
    }

    #[must_use]
    pub fn platform_id(&self) -> PlatformId {
        PlatformId::new("onebot/v11")
    }

    #[must_use]
    pub fn to_event_envelope(&self) -> EventEnvelope {
        let platform = self.platform_id();
        let sender = UserRef::new(platform.clone(), self.user_id().to_string())
            .with_display_name(self.nickname().to_string());
        let channel = match self.message_event() {
            MessageEvent::Private(p) => ChannelRef::direct(platform.clone(), p.user_id.to_string()),
            MessageEvent::Group(g) => ChannelRef::group(platform.clone(), g.group_id.to_string()),
        };
        let message_id = match self.message_event() {
            MessageEvent::Private(p) => p.message_id.to_string(),
            MessageEvent::Group(g) => g.message_id.to_string(),
        };
        let segments = to_kernel_segments(self.message());

        let message = KernelMessageEvent::new(sender, channel, self.text())
            .with_message_id(message_id)
            .with_segments(segments);

        EventEnvelope::new(BotId::new(self.self_id().to_string()), platform).with_message(message)
    }

    #[must_use]
    pub fn into_context(self, sender: Option<std::sync::Arc<dyn OutboundSender>>) -> Context {
        let envelope = self.to_event_envelope();
        Context::new(envelope, sender, self)
    }
}

fn message_payload(event: &MessageEvent) -> &Message {
    match event {
        MessageEvent::Private(private) => &private.message,
        MessageEvent::Group(group) => &group.message,
    }
}

fn to_kernel_segments(message: &Message) -> Vec<KernelMessageSegment> {
    match message {
        Message::String(text) => vec![KernelMessageSegment::text(text.clone())],
        Message::Segment(segment) => vec![to_kernel_segment(segment)],
        Message::Array(segments) => segments.iter().map(to_kernel_segment).collect(),
    }
}

fn to_kernel_segment(segment: &MessageSegment) -> KernelMessageSegment {
    match segment {
        MessageSegment::Text { text } => KernelMessageSegment::text(text.clone()),
        MessageSegment::At { qq } => KernelMessageSegment::Mention {
            user_id: qq.clone(),
        },
        MessageSegment::Image { file, url, .. } => KernelMessageSegment::Image {
            url: url.clone().unwrap_or_else(|| file.clone()),
        },
        MessageSegment::Record { file, url, .. } | MessageSegment::Video { file, url } => {
            KernelMessageSegment::Attachment {
                name: None,
                url: url.clone().or_else(|| Some(file.clone())),
                mime: None,
            }
        }
        other => KernelMessageSegment::Unknown {
            kind: serde_json::to_string(other).unwrap_or_else(|_| "unknown".to_string()),
            data: serde_json::Value::Null,
        },
    }
}

impl Ctx {
    async fn send_request(&self, req: ApiRequest) -> Result<()> {
        let json = serde_json::to_string(&req)?;
        self.outgoing_tx.send(json).await?;
        Ok(())
    }

    fn next_echo(&self, action: &str) -> String {
        let seq = self.echo_seq.fetch_add(1, Ordering::Relaxed);
        format!("ayiou:{}:{}:{}", action, self.user_id(), seq)
    }

    /// Call API without waiting for response
    pub async fn call(&self, action: &str, params: serde_json::Value) -> Result<()> {
        self.send_request(ApiRequest {
            action: action.to_string(),
            params,
            echo: None,
        })
        .await
    }

    /// Call API and wait for `OneBot` response by `echo`.
    pub async fn call_with_response(
        &self,
        action: &str,
        params: serde_json::Value,
    ) -> Result<ApiResponse> {
        let echo_value = serde_json::Value::String(self.next_echo(action));
        let echo = echo_key(&echo_value).ok_or_else(|| anyhow!("Invalid echo value"))?;
        let (tx, rx) = oneshot::channel();
        self.pending_api.insert(echo.clone(), tx);

        self.send_request(ApiRequest {
            action: action.to_string(),
            params,
            echo: Some(echo_value),
        })
        .await?;

        match tokio::time::timeout(API_TIMEOUT, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => {
                self.pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response channel closed for action `{action}`"
                ))
            }
            Err(_) => {
                self.pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response timed out for action `{action}` after {API_TIMEOUT:?}"
                ))
            }
        }
    }

    /// Call typed `OneBot` action without waiting for response.
    pub async fn call_action(&self, action: OneBotAction) -> Result<()> {
        self.send_request(action.into_request()).await
    }

    /// Call typed `OneBot` action and wait for response.
    pub async fn call_action_with_response(&self, action: OneBotAction) -> Result<ApiResponse> {
        let mut req = action.into_request();
        let action_name = req.action.clone();
        let echo_value = serde_json::Value::String(self.next_echo(&action_name));
        let echo = echo_key(&echo_value).ok_or_else(|| anyhow!("Invalid echo value"))?;
        let (tx, rx) = oneshot::channel();
        self.pending_api.insert(echo.clone(), tx);
        req.echo = Some(echo_value);

        self.send_request(req).await?;

        match tokio::time::timeout(API_TIMEOUT, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => {
                self.pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response channel closed for action `{action_name}`"
                ))
            }
            Err(_) => {
                self.pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response timed out for action `{action_name}` after {API_TIMEOUT:?}"
                ))
            }
        }
    }

    /// Call any custom `OneBot` action without waiting for response.
    pub async fn call_custom_action(
        &self,
        action: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<()> {
        self.call_action(OneBotAction::Custom {
            action: action.into(),
            params,
        })
        .await
    }

    /// Call any custom `OneBot` action and wait for response.
    pub async fn call_custom_action_with_response(
        &self,
        action: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<ApiResponse> {
        self.call_action_with_response(OneBotAction::Custom {
            action: action.into(),
            params,
        })
        .await
    }

    /// Send private message
    pub async fn send_private_msg(&self, user_id: i64, message: &Message) -> Result<()> {
        self.call_action(OneBotAction::SendPrivateMsg {
            user_id,
            message: message.clone(),
        })
        .await
    }

    /// Send private message and return `OneBot` message id.
    pub async fn send_private_msg_with_response(
        &self,
        user_id: i64,
        message: &Message,
    ) -> Result<SendMessageData> {
        let resp = self
            .call_action_with_response(OneBotAction::SendPrivateMsg {
                user_id,
                message: message.clone(),
            })
            .await?;
        resp.data_as_checked("send_private_msg")
    }

    /// Send group message
    pub async fn send_group_msg(&self, group_id: i64, message: &Message) -> Result<()> {
        self.call_action(OneBotAction::SendGroupMsg {
            group_id,
            message: message.clone(),
        })
        .await
    }

    /// Send group message and return `OneBot` message id.
    pub async fn send_group_msg_with_response(
        &self,
        group_id: i64,
        message: &Message,
    ) -> Result<SendMessageData> {
        let resp = self
            .call_action_with_response(OneBotAction::SendGroupMsg {
                group_id,
                message: message.clone(),
            })
            .await?;
        resp.data_as_checked("send_group_msg")
    }

    /// Kick group member
    pub async fn kick_group_member(&self, group_id: i64, user_id: i64) -> Result<()> {
        self.call_action(OneBotAction::SetGroupKick {
            group_id,
            user_id,
            reject_add_request: false,
        })
        .await
    }

    /// Delete/recall message
    pub async fn delete_msg(&self, message_id: i32) -> Result<()> {
        self.call_action(OneBotAction::DeleteMsg { message_id })
            .await
    }

    /// Get login info
    pub async fn get_login_info(&self) -> Result<ApiResponse> {
        self.call_action_with_response(OneBotAction::GetLoginInfo)
            .await
    }

    /// Get login info and decode typed data.
    pub async fn get_login_info_data(&self) -> Result<LoginInfoData> {
        let resp = self.get_login_info().await?;
        resp.data_as_checked("get_login_info")
    }

    /// Get group info
    pub async fn get_group_info(&self, group_id: i64) -> Result<ApiResponse> {
        self.call_action_with_response(OneBotAction::GetGroupInfo {
            group_id,
            no_cache: false,
        })
        .await
    }

    /// Get group info and decode typed data.
    pub async fn get_group_info_data(&self, group_id: i64) -> Result<GroupInfoData> {
        let resp = self.get_group_info(group_id).await?;
        resp.data_as_checked("get_group_info")
    }

    /// Get group member info
    pub async fn get_group_member_info(&self, group_id: i64, user_id: i64) -> Result<ApiResponse> {
        self.call_action_with_response(OneBotAction::GetGroupMemberInfo {
            group_id,
            user_id,
            no_cache: false,
        })
        .await
    }

    /// Get group member info and decode typed data.
    pub async fn get_group_member_info_data(
        &self,
        group_id: i64,
        user_id: i64,
    ) -> Result<GroupMemberInfoData> {
        let resp = self.get_group_member_info(group_id, user_id).await?;
        resp.data_as_checked("get_group_member_info")
    }

    /// Set group ban
    pub async fn set_group_ban(&self, group_id: i64, user_id: i64, duration: i64) -> Result<()> {
        self.call_action(OneBotAction::SetGroupBan {
            group_id,
            user_id,
            duration,
        })
        .await
    }
}
