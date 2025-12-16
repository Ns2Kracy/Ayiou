use anyhow::Result;
use async_trait::async_trait;
use ayiou::core::plugin::Command;
use ayiou::prelude::*;

/// Arguments for the /whoami command (empty)
#[derive(Debug, Default, Args)]
pub struct WhoAmIArgs;

#[async_trait]
impl Command for WhoAmIArgs {
    async fn run(self, ctx: Ctx) -> Result<()> {
        let user_id = ctx.user_id();
        let group_id = ctx.group_id();
        let nickname = ctx.nickname();

        let mut msg = format!("You are {} ({})\n", nickname, user_id);
        if let Some(gid) = group_id {
            msg.push_str(&format!("In Group: {}", gid));
        } else {
            msg.push_str("In Private Chat");
        }
        ctx.reply_text(msg).await?;
        Ok(())
    }
}
