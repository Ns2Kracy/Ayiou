use ayiou::prelude::*;
use ayiou::core::plugin::Command; // Import Command trait
use anyhow::Result;
use async_trait::async_trait;

/// Arguments for the /echo command
#[derive(Debug, Args)]
pub struct EchoArgs {
    #[arg(rest)]
    pub content: String,
}

#[async_trait]
impl Command for EchoArgs {
    async fn run(self, ctx: Ctx) -> Result<()> {
        ctx.reply_text(format!("Echo: {}", self.content)).await?;
        Ok(())
    }
}
