use std::marker::PhantomData;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::adapter::onebot::v11::model::Message;
use crate::core::scheduler::Scheduler;
use crate::core::storage::Store;

#[async_trait]
pub trait MessageSender: Send + Sync {
    async fn send_private_message(&self, user_id: i64, message: Message) -> Result<()>;
    async fn send_group_message(&self, group_id: i64, message: Message) -> Result<()>;

    async fn send_private_text(&self, user_id: i64, text: &str) -> Result<()> {
        self.send_private_message(user_id, Message::String(text.to_string()))
            .await
    }

    async fn send_group_text(&self, group_id: i64, text: &str) -> Result<()> {
        self.send_group_message(group_id, Message::String(text.to_string()))
            .await
    }
}

#[derive(Clone)]
pub struct PluginHost<C> {
    scheduler: Arc<dyn Scheduler>,
    store: Arc<dyn Store>,
    sender: Option<Arc<dyn MessageSender>>,
    _marker: PhantomData<fn() -> C>,
}

impl<C> PluginHost<C> {
    pub fn new(
        scheduler: Arc<dyn Scheduler>,
        store: Arc<dyn Store>,
        sender: Option<Arc<dyn MessageSender>>,
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

    pub fn sender(&self) -> Option<Arc<dyn MessageSender>> {
        self.sender.clone()
    }

    pub fn require_sender(&self) -> Result<Arc<dyn MessageSender>> {
        self.sender
            .clone()
            .ok_or_else(|| anyhow!("adapter does not provide proactive message sending"))
    }
}
