use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::core::{
    model::{MessageSegment, OutboundMessage, OutboundReceipt},
    plugin_host::OutboundSender,
};

#[derive(Clone)]
pub struct ConsoleSender {
    outgoing_tx: mpsc::Sender<String>,
}

impl ConsoleSender {
    #[must_use]
    pub const fn new(outgoing_tx: mpsc::Sender<String>) -> Self {
        Self { outgoing_tx }
    }
}

#[async_trait]
impl OutboundSender for ConsoleSender {
    async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt> {
        let mut rendered = String::new();

        for segment in message.segments {
            match segment {
                MessageSegment::Text { text } => rendered.push_str(&text),
                MessageSegment::Mention { user_id } => {
                    if !rendered.is_empty() {
                        rendered.push(' ');
                    }
                    rendered.push('@');
                    rendered.push_str(&user_id);
                }
                MessageSegment::Image { url } => {
                    if !rendered.is_empty() {
                        rendered.push(' ');
                    }
                    rendered.push_str("[image:");
                    rendered.push_str(&url);
                    rendered.push(']');
                }
                MessageSegment::Attachment { name, .. } => {
                    if !rendered.is_empty() {
                        rendered.push(' ');
                    }
                    rendered.push_str("[attachment:");
                    rendered.push_str(name.as_deref().unwrap_or("unnamed"));
                    rendered.push(']');
                }
                MessageSegment::Unknown { kind, .. } => {
                    if !rendered.is_empty() {
                        rendered.push(' ');
                    }
                    rendered.push('[');
                    rendered.push_str(&kind);
                    rendered.push(']');
                }
            }
        }

        self.outgoing_tx.send(rendered).await?;
        Ok(OutboundReceipt::default())
    }
}
