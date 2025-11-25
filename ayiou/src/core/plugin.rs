use crate::core::{context::Ctx, event::Event};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct PluginMeta {
    pub name: String,
    pub description: String,
    pub version: String,
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn meta(&self) -> PluginMeta;

    async fn call(&self, event: Arc<Event>, ctx: Arc<Ctx>) -> Result<()>;
}
