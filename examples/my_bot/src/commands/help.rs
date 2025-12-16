use ayiou::prelude::*;
use anyhow::Result;
use crate::plugin::DemoPlugin; // Import DemoPlugin to access help_text

pub async fn handle(ctx: Ctx) -> Result<()> {
    // This creates a circular dependency if help.rs depends on plugin.rs and plugin.rs depends on help.rs
    // To solve this, we might need to expose help_text differently or just accept the circularity (Rust handles it within crates usually)
    // Or we can just print a static string for now to be safe, but let's try to use the generated one.
    let help = DemoPlugin::help_text();
    ctx.reply_text(help).await?;
    Ok(())
}
