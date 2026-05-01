use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::core::model::{ChannelRef, OutboundMessage, OutboundReceipt};

#[async_trait]
pub trait OutboundSender: Send + Sync {
    async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt>;
}

pub use OutboundSender as MessageSender;

pub struct PluginHost<C> {
    sender: Option<Arc<dyn OutboundSender>>,
    _marker: PhantomData<fn() -> C>,
}

impl<C> Clone for PluginHost<C> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C> PluginHost<C> {
    #[must_use]
    pub fn new(sender: Option<Arc<dyn OutboundSender>>) -> Self {
        Self {
            sender,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub fn sender(&self) -> Option<Arc<dyn OutboundSender>> {
        self.sender.clone()
    }

    pub fn require_sender(&self) -> Result<Arc<dyn OutboundSender>> {
        self.sender
            .clone()
            .ok_or_else(|| anyhow!("adapter does not provide proactive message sending"))
    }

    pub async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt> {
        self.require_sender()?.send(message).await
    }

    pub async fn send_text(
        &self,
        target: ChannelRef,
        text: impl Into<String>,
    ) -> Result<OutboundReceipt> {
        self.send(OutboundMessage::text(target, text)).await
    }
}
