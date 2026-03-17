use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::core::model::{ChannelRef, OutboundMessage, OutboundReceipt};
use crate::core::scheduler::Scheduler;
use crate::core::storage::Store;

#[async_trait]
pub trait OutboundSender: Send + Sync {
    async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt>;
}

pub use OutboundSender as MessageSender;

#[derive(Clone)]
pub struct PluginHost<C> {
    scheduler: Arc<dyn Scheduler>,
    store: Arc<dyn Store>,
    sender: Option<Arc<dyn OutboundSender>>,
    _marker: PhantomData<fn() -> C>,
}

impl<C> PluginHost<C> {
    pub fn new(
        scheduler: Arc<dyn Scheduler>,
        store: Arc<dyn Store>,
        sender: Option<Arc<dyn OutboundSender>>,
    ) -> Self {
        Self {
            scheduler,
            store,
            sender,
            _marker: PhantomData,
        }
    }

    pub fn scheduler(&self) -> Arc<dyn Scheduler> {
        self.scheduler.clone()
    }

    pub fn store(&self) -> Arc<dyn Store> {
        self.store.clone()
    }

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
