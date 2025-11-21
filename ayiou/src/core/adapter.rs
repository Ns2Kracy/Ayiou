use crate::core::Context;
use anyhow::Result;
use async_trait::async_trait;

/// An Adapter bridges the Driver and the Core App.
/// It translates DriverEvents to Core Events and manages Bots.
#[async_trait]
pub trait Adapter: Send + Sync {
    /// Bind the adapter to the global context.
    /// This should be called before `run`.
    fn bind(&mut self, context: Context);

    /// Start the adapter.
    /// This usually involves starting the underlying driver and listening for driver events.
    async fn run(&self) -> Result<()>;
}
