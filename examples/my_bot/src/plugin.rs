use ayiou::prelude::*;

// ============================================================================
// Derive Macro Plugins (Simpler API)
// ============================================================================

#[derive(Plugin)]
#[plugin(
    name = "echo",
    command = "echo",
    prefix = "/",
    description = "Repeats your message"
)]
pub struct EchoPlugin;

impl EchoPlugin {
    pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
        let content = ctx.command_args().unwrap_or_default();
        if content.is_empty() {
            ctx.reply_text("Usage: /echo <text>").await?;
            return Ok(());
        }

        ctx.reply_text(format!("Echo: {}", content)).await?;
        Ok(())
    }
}

#[derive(Plugin)]
#[plugin(name = "add", command = "add", description = "Adds two numbers")]
pub struct AddPlugin;

impl AddPlugin {
    pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
        let args = ctx.command_args().unwrap_or_default();
        let parts: Vec<&str> = args.split_whitespace().collect();

        if parts.len() < 2 {
            ctx.reply_text("Usage: /add <a> <b>").await?;
            return Ok(());
        }

        let a: i32 = parts[0].parse().unwrap_or(0);
        let b: i32 = parts[1].parse().unwrap_or(0);
        ctx.reply_text(format!("{} + {} = {}", a, b, a + b)).await?;
        Ok(())
    }
}

#[derive(Plugin)]
#[plugin(name = "whoami", command = "whoami", description = "Shows user info")]
pub struct WhoamiPlugin;

impl WhoamiPlugin {
    pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
        let user_id = ctx.user_id();
        let nickname = ctx.nickname();
        let mut msg = format!("You are {} ({})", nickname, user_id);
        if let Some(gid) = ctx.group_id() {
            msg.push_str(&format!("\nIn Group: {}", gid));
        } else {
            msg.push_str("\nIn Private Chat");
        }
        ctx.reply_text(msg).await?;
        Ok(())
    }
}

#[derive(Plugin)]
#[plugin(name = "guess", command = "guess", description = "Guessing game")]
pub struct GuessPlugin;

impl GuessPlugin {
    pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
        ctx.reply_text("Session functionality is currently disabled.")
            .await?;
        Ok(())
    }
}

// ============================================================================
// Regex-based Plugins (Match on message pattern)
// ============================================================================

#[derive(Plugin)]
#[plugin(
    name = "url_detector",
    regex = r"https?://\S+",
    description = "Detects URLs in messages"
)]
pub struct UrlDetectorPlugin;

impl UrlDetectorPlugin {
    pub async fn execute(&self, ctx: &Ctx) -> anyhow::Result<()> {
        let text = ctx.text();
        // Extract URLs from message
        let re = self.regex();
        let urls: Vec<&str> = re.find_iter(&text).map(|m| m.as_str()).collect();

        if !urls.is_empty() {
            ctx.reply_text(format!("Found URLs: {}", urls.join(", ")))
                .await?;
        }
        Ok(())
    }
}

// ============================================================================
// Attribute Macro Plugin (v0.4 command DX)
// ============================================================================

#[derive(Default)]
pub struct ToolboxPlugin;

#[bot_plugin(
    name = "toolbox",
    description = "Attribute macro command plugin",
    prefix = "/"
)]
impl ToolboxPlugin {
    #[command(name = "mul", alias = "times")]
    pub async fn multiply(&self, ctx: &Ctx, a: i64, b: i64) -> anyhow::Result<()> {
        ctx.reply_text(format!("{} * {} = {}", a, b, a * b)).await?;
        Ok(())
    }

    #[command]
    pub async fn ping(&self, ctx: &Ctx, target: Option<String>) -> anyhow::Result<()> {
        if let Some(target) = target {
            ctx.reply_text(format!("pong {}", target)).await?;
        } else {
            ctx.reply_text("pong").await?;
        }
        Ok(())
    }

    #[command(name = "say")]
    pub async fn say(&self, ctx: &Ctx, content: String) -> anyhow::Result<()> {
        if content.is_empty() {
            ctx.reply_text("Usage: /say <content>").await?;
            return Ok(());
        }

        ctx.reply_text(content).await?;
        Ok(())
    }
}
