use std::{any::Any, sync::Arc};

use anyhow::{Result, anyhow};

use crate::core::{
    adapter::MsgContext,
    model::{EventEnvelope, MessageEvent, OutboundMessage},
    plugin_host::OutboundSender,
};

#[derive(Clone)]
pub struct Context {
    envelope: EventEnvelope,
    outbound: Option<Arc<dyn OutboundSender>>,
    extension: Arc<dyn Any + Send + Sync>,
}

impl Context {
    pub fn new<T>(
        envelope: EventEnvelope,
        outbound: Option<Arc<dyn OutboundSender>>,
        extension: T,
    ) -> Self
    where
        T: Any + Send + Sync,
    {
        Self {
            envelope,
            outbound,
            extension: Arc::new(extension),
        }
    }

    #[must_use]
    pub const fn event(&self) -> &EventEnvelope {
        &self.envelope
    }

    #[must_use]
    pub const fn message(&self) -> Option<&MessageEvent> {
        self.envelope.message()
    }

    #[must_use]
    pub fn extension<T>(&self) -> Option<&T>
    where
        T: Any + Send + Sync + 'static,
    {
        self.extension.as_ref().downcast_ref::<T>()
    }

    pub async fn reply(&self, message: OutboundMessage) -> Result<()> {
        let sender = self
            .outbound
            .clone()
            .ok_or_else(|| anyhow!("adapter does not provide proactive message sending"))?;

        sender.send(message).await?;
        Ok(())
    }

    pub async fn reply_text(&self, text: impl Into<String>) -> Result<()> {
        let message = self
            .message()
            .ok_or_else(|| anyhow!("current event does not carry a message context"))?;
        self.reply(OutboundMessage::text(message.channel.clone(), text))
            .await
    }
}

impl MsgContext for Context {
    fn text(&self) -> String {
        self.message()
            .map(|msg| msg.text.clone())
            .unwrap_or_default()
    }

    fn user_id(&self) -> String {
        self.message()
            .map(|msg| msg.sender.user_id().to_string())
            .unwrap_or_default()
    }

    fn group_id(&self) -> Option<String> {
        self.message().and_then(|msg| match msg.channel.kind() {
            crate::core::model::ChannelKind::Group => Some(msg.channel.channel_id().to_string()),
            crate::core::model::ChannelKind::Direct | crate::core::model::ChannelKind::Channel => {
                None
            }
        })
    }
}
