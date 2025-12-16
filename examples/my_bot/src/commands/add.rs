use ayiou::prelude::*;
use ayiou::core::plugin::Command;
use anyhow::Result;
use async_trait::async_trait;

/// Arguments for the /add command
#[derive(Debug, Args)]
pub struct AddArgs {
    pub a: i32,
    pub b: i32,
}

#[async_trait]
impl Command for AddArgs {
    async fn run(self, ctx: Ctx) -> Result<()> {
        let sum = self.a + self.b;
        ctx.reply_text(format!("{} + {} = {}", self.a, self.b, sum)).await?;
        Ok(())
    }
}
