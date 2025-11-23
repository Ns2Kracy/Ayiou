use crate::core::adapter::Adapter;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// A Driver is responsible for low-level I/O (Network, Stdin, etc).
/// It passes raw events to an adapter.
#[async_trait]
pub trait Driver: Send + Sync {
    /// Run the driver's main loop.
    /// The driver should listen for incoming raw data and pass it to the provided adapter's `handle_raw_event` method.
    async fn run(&self, adapter: Arc<dyn Adapter>) -> Result<()>;

    /// Send raw data back to the client/user.
    async fn send(&self, content: String) -> Result<()>;
}
