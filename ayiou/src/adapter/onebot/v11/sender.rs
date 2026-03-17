use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::adapter::onebot::v11::model::{Message, OneBotAction};
use crate::core::{
    model::{
        ChannelKind, MessageSegment as KernelMessageSegment, OutboundMessage, OutboundReceipt,
    },
    plugin_host::OutboundSender,
};

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
impl OutboundSender for OneBotSender {
    async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt> {
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

fn to_onebot_message(segments: Vec<KernelMessageSegment>) -> Message {
    let mut out = Vec::with_capacity(segments.len());

    for segment in segments {
        match segment {
            KernelMessageSegment::Text { text } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::Text { text })
            }
            KernelMessageSegment::Mention { user_id } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::At { qq: user_id })
            }
            KernelMessageSegment::Image { url } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::Image {
                    file: url,
                    image_type: None,
                    url: None,
                })
            }
            KernelMessageSegment::Attachment { url, .. } => {
                out.push(crate::adapter::onebot::v11::model::MessageSegment::Record {
                    file: url.unwrap_or_default(),
                    magic: None,
                    url: None,
                })
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
