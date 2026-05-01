use std::{
    sync::{
        Arc, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use dashmap::DashMap;
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};

use crate::adapter::onebot::v11::model::{
    ApiResponse, LoginInfoData, Message, OneBotAction, echo_key,
};
use crate::core::{
    model::{
        ChannelKind, MessageSegment as KernelMessageSegment, OutboundMessage, OutboundReceipt,
    },
    plugin_host::OutboundSender,
};

const ONEBOT_FORWARD_KIND: &str = "onebot_v11_forward";
const API_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Deserialize)]
struct ForwardBundle {
    nodes: Vec<ForwardNode>,
}

#[derive(Debug, Clone, Deserialize)]
struct ForwardNode {
    user_id: i64,
    nickname: String,
    content: Vec<KernelMessageSegment>,
}

#[derive(Clone)]
pub struct OneBotSender {
    outgoing_tx: mpsc::Sender<String>,
    pending_api: Option<Arc<DashMap<String, oneshot::Sender<ApiResponse>>>>,
    echo_seq: Option<Arc<AtomicU64>>,
    profile: Arc<RwLock<Option<LoginInfoData>>>,
}

impl OneBotSender {
    #[must_use]
    pub fn new(outgoing_tx: mpsc::Sender<String>) -> Self {
        Self {
            outgoing_tx,
            pending_api: None,
            echo_seq: None,
            profile: Arc::new(RwLock::new(None)),
        }
    }

    pub const fn with_runtime(
        outgoing_tx: mpsc::Sender<String>,
        pending_api: Arc<DashMap<String, oneshot::Sender<ApiResponse>>>,
        echo_seq: Arc<AtomicU64>,
        profile: Arc<RwLock<Option<LoginInfoData>>>,
    ) -> Self {
        Self {
            outgoing_tx,
            pending_api: Some(pending_api),
            echo_seq: Some(echo_seq),
            profile,
        }
    }

    #[must_use]
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

    fn next_echo(&self, action: &str) -> Result<String> {
        let Some(echo_seq) = self.echo_seq.as_ref() else {
            return Err(anyhow!(
                "OneBotSender does not support response-mapped actions"
            ));
        };
        let seq = echo_seq.fetch_add(1, Ordering::Relaxed);
        Ok(format!("ayiou-sender:{action}:{seq}"))
    }

    async fn call_action_with_response(&self, action: OneBotAction) -> Result<ApiResponse> {
        let Some(pending_api) = self.pending_api.as_ref() else {
            return Err(anyhow!(
                "OneBotSender does not support response-mapped actions"
            ));
        };

        let mut req = action.into_request();
        let action_name = req.action.clone();
        let echo_value = serde_json::Value::String(self.next_echo(&action_name)?);
        let echo = echo_key(&echo_value).ok_or_else(|| anyhow!("Invalid echo value"))?;
        let (tx, rx) = oneshot::channel();
        pending_api.insert(echo.clone(), tx);
        req.echo = Some(echo_value);

        let raw = serde_json::to_string(&req)?;
        self.outgoing_tx.send(raw).await?;

        match tokio::time::timeout(API_TIMEOUT, rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => {
                pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response channel closed for action `{action_name}`"
                ))
            }
            Err(_) => {
                pending_api.remove(&echo);
                Err(anyhow!(
                    "OneBot response timed out for action `{action_name}` after {API_TIMEOUT:?}"
                ))
            }
        }
    }

    async fn resolve_profile(&self) -> Option<LoginInfoData> {
        let cached_profile = self.profile.read().expect("profile lock").clone();
        if let Some(profile) = cached_profile {
            return Some(profile);
        }

        let response = self
            .call_action_with_response(OneBotAction::GetLoginInfo)
            .await
            .ok()?;
        let profile = response
            .data_as_checked::<LoginInfoData>("get_login_info")
            .ok()?;
        *self.profile.write().expect("profile lock") = Some(profile.clone());
        Some(profile)
    }
}

#[async_trait]
impl OutboundSender for OneBotSender {
    async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt> {
        if let Some(bundle) = extract_forward_bundle(&message.segments) {
            let profile = self.resolve_profile().await;
            match message.target.kind() {
                ChannelKind::Direct => {
                    let user_id = message.target.channel_id().parse::<i64>()?;
                    self.send_action(OneBotAction::Custom {
                        action: "send_private_forward_msg".to_string(),
                        params: serde_json::json!({
                            "user_id": user_id,
                            "messages": to_onebot_forward_nodes(bundle.nodes, profile.as_ref()),
                        }),
                    })
                    .await?;
                }
                ChannelKind::Group | ChannelKind::Channel => {
                    let group_id = message.target.channel_id().parse::<i64>()?;
                    self.send_action(OneBotAction::Custom {
                        action: "send_group_forward_msg".to_string(),
                        params: serde_json::json!({
                            "group_id": group_id,
                            "messages": to_onebot_forward_nodes(bundle.nodes, profile.as_ref()),
                        }),
                    })
                    .await?;
                }
            }

            return Ok(OutboundReceipt::default());
        }

        let message_body = to_onebot_message(message.segments);

        match message.target.kind() {
            ChannelKind::Direct => {
                let user_id = message.target.channel_id().parse::<i64>()?;
                self.send_action(OneBotAction::SendPrivateMsg {
                    user_id,
                    message: message_body,
                })
                .await?;
            }
            ChannelKind::Group | ChannelKind::Channel => {
                let group_id = message.target.channel_id().parse::<i64>()?;
                self.send_action(OneBotAction::SendGroupMsg {
                    group_id,
                    message: message_body,
                })
                .await?;
            }
        }

        Ok(OutboundReceipt::default())
    }
}

fn extract_forward_bundle(segments: &[KernelMessageSegment]) -> Option<ForwardBundle> {
    match segments {
        [KernelMessageSegment::Unknown { kind, data }] if kind == ONEBOT_FORWARD_KIND => {
            serde_json::from_value(data.clone()).ok()
        }
        _ => None,
    }
}

fn to_onebot_forward_nodes(
    nodes: Vec<ForwardNode>,
    profile: Option<&LoginInfoData>,
) -> Vec<crate::adapter::onebot::v11::model::MessageSegment> {
    nodes
        .into_iter()
        .map(
            |node| crate::adapter::onebot::v11::model::MessageSegment::Node {
                data: crate::adapter::onebot::v11::model::NodeData::Custom {
                    user_id: profile.map_or(node.user_id, |item| item.user_id),
                    nickname: profile
                        .map(|item| item.nickname.clone())
                        .unwrap_or(node.nickname),
                    content: Box::new(to_onebot_message(node.content)),
                },
            },
        )
        .collect()
}

fn to_onebot_message(segments: Vec<KernelMessageSegment>) -> Message {
    let mut out = Vec::with_capacity(segments.len());

    for segment in segments {
        match segment {
            KernelMessageSegment::Text { text } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::Text { text });
            }
            KernelMessageSegment::Mention { user_id } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::At { qq: user_id });
            }
            KernelMessageSegment::Image { url } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::Image {
                    file: url,
                    image_type: None,
                    url: None,
                });
            }
            KernelMessageSegment::Attachment { url, .. } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::Record {
                    file: url.unwrap_or_default(),
                    magic: None,
                    url: None,
                });
            }
            KernelMessageSegment::Unknown { .. } => {}
        }
    }

    match out.len() {
        0 => Message::String(String::new()),
        1 => Message::Segment(out.remove(0)),
        _ => Message::Array(out),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock, atomic::AtomicU64};

    use dashmap::DashMap;
    use serde_json::Value;
    use tokio::sync::oneshot;

    use super::*;
    use crate::core::model::{ChannelRef, MessageSegment, OutboundMessage};

    #[tokio::test]
    async fn sender_uses_group_forward_action_for_forward_bundle() {
        let (sender, mut rx) = OneBotSender::test_pair();
        let message = OutboundMessage::new(
            ChannelRef::group("onebot/v11", "123456"),
            vec![MessageSegment::Unknown {
                kind: ONEBOT_FORWARD_KIND.to_string(),
                data: serde_json::json!({
                    "nodes": [
                        {
                            "user_id": 42,
                            "nickname": "B站直播订阅",
                            "content": [
                                {"Text": {"text": "B站直播订阅\n当前会话共 1 条"}},
                                {"Image": {"url": "https://example.com/cover.jpg"}}
                            ]
                        }
                    ]
                }),
            }],
        );

        sender.send(message).await.expect("send should succeed");

        let raw = rx.recv().await.expect("request should be sent");
        let request: Value = serde_json::from_str(&raw).expect("request json should decode");
        assert_eq!(request["action"], Value::from("send_group_forward_msg"));
        assert_eq!(request["params"]["group_id"], Value::from(123_456));
        assert_eq!(
            request["params"]["messages"][0]["type"],
            Value::from("node")
        );
        assert_eq!(
            request["params"]["messages"][0]["data"]["nickname"],
            Value::from("B站直播订阅")
        );
        assert_eq!(
            request["params"]["messages"][0]["data"]["content"][0]["type"],
            Value::from("text")
        );
        assert_eq!(
            request["params"]["messages"][0]["data"]["content"][1]["type"],
            Value::from("image")
        );
    }

    #[tokio::test]
    async fn sender_keeps_regular_group_messages_as_send_group_msg() {
        let (sender, mut rx) = OneBotSender::test_pair();
        let message = OutboundMessage::text(ChannelRef::group("onebot/v11", "123456"), "hello");

        sender.send(message).await.expect("send should succeed");

        let raw = rx.recv().await.expect("request should be sent");
        let request: Value = serde_json::from_str(&raw).expect("request json should decode");
        assert_eq!(request["action"], Value::from("send_group_msg"));
    }

    #[tokio::test]
    async fn sender_uses_login_info_as_forward_node_identity_when_available() {
        let (tx, mut rx) = mpsc::channel(8);
        let pending_api = Arc::new(DashMap::<String, oneshot::Sender<ApiResponse>>::new());
        let profile = Arc::new(RwLock::new(None));
        let sender = OneBotSender::with_runtime(
            tx,
            pending_api.clone(),
            Arc::new(AtomicU64::new(1)),
            profile,
        );
        let message = OutboundMessage::new(
            ChannelRef::group("onebot/v11", "123456"),
            vec![MessageSegment::Unknown {
                kind: ONEBOT_FORWARD_KIND.to_string(),
                data: serde_json::json!({
                    "nodes": [
                        {
                            "user_id": 42,
                            "nickname": "原始昵称",
                            "content": [
                                {"Text": {"text": "hello"}}
                            ]
                        }
                    ]
                }),
            }],
        );

        let send_task = tokio::spawn({
            let sender = sender.clone();
            async move { sender.send(message).await.expect("send should succeed") }
        });

        let raw_login = rx.recv().await.expect("login request should be sent");
        let login_request: Value =
            serde_json::from_str(&raw_login).expect("login request json should decode");
        assert_eq!(login_request["action"], Value::from("get_login_info"));
        let echo = login_request["echo"].clone();
        let echo_key = echo.to_string();
        let (_, reply_tx) = pending_api
            .remove(&echo_key)
            .expect("pending login response should exist");
        reply_tx
            .send(ApiResponse {
                status: "ok".to_string(),
                retcode: 0,
                data: serde_json::json!({
                    "user_id": 31_415_926,
                    "nickname": "AyiouBot"
                }),
                echo: Some(echo),
            })
            .expect("login response should send");

        let raw_forward = rx.recv().await.expect("forward request should be sent");
        let forward_request: Value =
            serde_json::from_str(&raw_forward).expect("forward request json should decode");
        assert_eq!(
            forward_request["action"],
            Value::from("send_group_forward_msg")
        );
        assert_eq!(
            forward_request["params"]["messages"][0]["data"]["user_id"],
            Value::from(31_415_926)
        );
        assert_eq!(
            forward_request["params"]["messages"][0]["data"]["nickname"],
            Value::from("AyiouBot")
        );

        send_task.await.expect("send task should finish");
    }
}
