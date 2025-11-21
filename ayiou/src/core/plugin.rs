use crate::core::Context;
use crate::core::event::EventHandler;
use anyhow::Result;
use async_trait::async_trait;

/// The Plugin trait.
/// In a real implementation, macros would generate the implementation of this trait.
#[async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &'static str;

    /// Called when the plugin is loaded. Great for initializing DB tables or config.
    async fn on_load(&self, _ctx: &Context) -> Result<()> {
        Ok(())
    }

    /// Called when the app is starting, after all plugins are loaded.
    async fn on_start(&self, _ctx: &Context) -> Result<()> {
        Ok(())
    }

    /// Called when the app is stopping.
    async fn on_stop(&self, _ctx: &Context) -> Result<()> {
        Ok(())
    }

    /// Return a list of event handlers registered by this plugin.
    fn handlers(&self) -> Vec<Box<dyn EventHandler>>;
}
