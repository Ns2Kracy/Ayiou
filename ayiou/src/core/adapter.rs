use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::core::context::Context;
use crate::core::plugin::{Capability, OutboundSender};

/// High-level adapter trait.
///
/// Adapter is responsible for protocol translation:
/// - raw inbound packet -> context/event
/// - context action -> raw outbound packet
pub struct AdapterRuntime {
    pub events: mpsc::Receiver<Context>,
    pub sender: Option<Arc<dyn OutboundSender>>,
    pub capabilities: Vec<Capability>,
}

#[async_trait]
pub trait Adapter: Send + Sync + 'static {
    /// Start adapter and return normalized events plus outbound facilities.
    async fn start(self) -> AdapterRuntime;
}
