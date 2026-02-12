use std::{
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
    ApiRequest, ApiResponse, GroupInfoData, GroupMemberInfoData, GroupMessageEvent, LoginInfoData,
    Message, MessageEvent, MessageSegment, OneBotAction, OneBotEvent, PrivateMessageEvent,
    SendMessageData, echo_key,
};
use crate::core::plugin::parse_command_line;

/// Message event type
#[derive(Clone)]
pub enum MsgEvent {
    Private(Arc<PrivateMessageEvent>),
    Group(Arc<GroupMessageEvent>),
}

use crate::core::adapter::MsgContext;

const DEFAULT_COMMAND_PREFIXES: [&str; 3] = ["/", "!", "."];
const API_TIMEOUT: Duration = Duration::from_secs(10);

/// Message context
#[derive(Clone)]
pub struct Ctx {
    event: Arc<OneBotEvent>,
    msg: MsgEvent,
    outgoing_tx: mpsc::Sender<String>,
    pending_api: Arc<DashMap<String, oneshot::Sender<ApiResponse>>>,
    echo_seq: Arc<AtomicU64>,
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
    pub fn new(
        event: Arc<OneBotEvent>,
        outgoing_tx: mpsc::Sender<String>,
        pending_api: Arc<DashMap<String, oneshot::Sender<ApiResponse>>>,
        echo_seq: Arc<AtomicU64>,
    ) -> Option<Self> {
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
            pending_api,
            echo_seq,
        })
    }

    /// Get raw OneBot event
    #[inline]
    pub fn event(&self) -> &OneBotEvent {
        &self.event
    }

    /// Parse command name with default prefixes (/, !, .)
    pub fn command_name(&self) -> Option<String> {
        self.command_name_with_prefixes(&DEFAULT_COMMAND_PREFIXES)
    }

    /// Parse command arguments with default prefixes (/, !, .)
    pub fn command_args(&self) -> Option<String> {
        self.command_args_with_prefixes(&DEFAULT_COMMAND_PREFIXES)
    }

    /// Parse command name with custom prefixes
    pub fn command_name_with_prefixes(&self, prefixes: &[&str]) -> Option<String> {
        let text = self.text();
        parse_command_line(&text, prefixes).map(|line| line.command().to_string())
    }

    /// Parse command arguments with custom prefixes
    pub fn command_args_with_prefixes(&self, prefixes: &[&str]) -> Option<String> {
        let text = self.text();
        parse_command_line(&text, prefixes).map(|line| line.args().to_string())
    }

    /// Get plain text from message
    pub fn text(&self) -> String {
        let message = match &self.msg {
            MsgEvent::Private(p) => &p.message,
            MsgEvent::Group(g) => &g.message,
        };

        match message {
            Message::String(s) => s.trim().to_string(),
            Message::Segment(seg) => match seg {
                MessageSegment::Text { text } => text.trim().to_string(),
                _ => String::new(),
            },
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
        }
        Ok(())
    }

    /// Reply with text
    pub async fn reply_text(&self, text: impl Into<String>) -> Result<()> {
        self.reply(Message::String(text.into())).await
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

    /// Call API and wait for OneBot response by `echo`.
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
                    "OneBot response channel closed for action `{}`",
                    action
                ))
            }
            Err(_) => {
                self.pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response timed out for action `{}` after {:?}",
                    action,
                    API_TIMEOUT
                ))
            }
        }
    }

    /// Call typed OneBot action without waiting for response.
    pub async fn call_action(&self, action: OneBotAction) -> Result<()> {
        self.send_request(action.into_request()).await
    }

    /// Call typed OneBot action and wait for response.
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
                    "OneBot response channel closed for action `{}`",
                    action_name
                ))
            }
            Err(_) => {
                self.pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response timed out for action `{}` after {:?}",
                    action_name,
                    API_TIMEOUT
                ))
            }
        }
    }

    /// Call any custom OneBot action without waiting for response.
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

    /// Call any custom OneBot action and wait for response.
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

    /// Send private message and return OneBot message id.
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

    /// Send group message and return OneBot message id.
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
