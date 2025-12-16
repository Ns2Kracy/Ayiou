use ayiou::prelude::*;
use ayiou::core::plugin::Command;
use anyhow::Result;
use async_trait::async_trait;

/// Arguments for the /email command
#[derive(Debug, Args)]
pub struct EmailArgs {
    #[arg(regex = r"^[\w\-\.]+@([\w-]+\.)+[\w-]{2,4}$")]
    pub email: RegexValidated,
}

#[async_trait]
impl Command for EmailArgs {
    async fn run(self, ctx: Ctx) -> Result<()> {
        ctx.reply_text(format!("Valid email received: {}", self.email)).await?;
        Ok(())
    }
}