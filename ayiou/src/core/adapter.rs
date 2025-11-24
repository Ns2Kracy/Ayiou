use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use crate::core::{TargetType, driver::Driver};

pub trait AdapterClone {
    fn clone_box(&self) -> Box<dyn Adapter>;
}

impl<T> AdapterClone for T
where
    T: 'static + Adapter + Clone,
{
    fn clone_box(&self) -> Box<dyn Adapter> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Adapter> {
    fn clone(&self) -> Box<dyn Adapter> {
        self.clone_box()
    }
}

/// An Adapter bridges the Driver and the Core App, and now also sends messages.
#[async_trait]
pub trait Adapter: Send + Sync + AdapterClone {
    /// Returns the name of the adapter (e.g., "onebot").
    fn name(&self) -> &'static str;

    /// Injects the driver dependency into the adapter instance.
    fn set_driver(&mut self, driver: Arc<dyn Driver>);

    /// Handles a raw event string from a driver and translates it into a core event.
    async fn handle(&self, raw_event: String) -> Result<()>;

    /// Serializes a generic `send_message` action into a platform-specific raw string.
    fn serialize(&self, target_id: &str, target_type: TargetType, content: &str) -> Result<String>;

    /// Send a text message to a user or group.
    async fn send_message(
        &self,
        target_id: &str,
        target_type: TargetType,
        content: &str,
    ) -> Result<String>;
}
