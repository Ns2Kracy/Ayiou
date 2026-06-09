use std::{any::Any, borrow::Cow, sync::Arc};

use anyhow::{Result, anyhow};

use crate::core::{
    model::{EventEnvelope, MessageEvent, OutboundMessage},
    plugin::OutboundSender,
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
    pub fn text(&self) -> Cow<'_, str> {
        self.message()
            .map(|msg| Cow::Borrowed(msg.text.as_str()))
            .unwrap_or_else(|| Cow::Borrowed(""))
    }

    #[must_use]
    pub fn user_id(&self) -> Cow<'_, str> {
        self.message()
            .map(|msg| Cow::Borrowed(msg.sender.user_id()))
            .unwrap_or_else(|| Cow::Borrowed(""))
    }

    #[must_use]
    pub fn group_id(&self) -> Option<Cow<'_, str>> {
        self.message().and_then(|msg| match msg.channel.kind() {
            crate::core::model::ChannelKind::Group => Some(Cow::Borrowed(msg.channel.channel_id())),
            crate::core::model::ChannelKind::Direct | crate::core::model::ChannelKind::Channel => {
                None
            }
        })
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
            .as_ref()
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
