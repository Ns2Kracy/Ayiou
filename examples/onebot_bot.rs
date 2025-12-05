use async_trait::async_trait;
use ayiou::{
    AyiouBot,
    core::{Ctx, Plugin, PluginMetadata},
};
use once_cell::sync::Lazy;
use regex::Regex;

struct Ping;
struct Hello;
struct Echo;
struct PrivateHelp;
struct Admin;

static HELLO_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(hi|hello|你好)").unwrap());

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    AyiouBot::new()
        .plugin(Ping)
        .plugin(Hello)
        .plugin(Echo)
        .plugin(PrivateHelp)
        .plugin(Admin)
        .connect("ws://10.126.126.1:3001")
        .run()
        .await;
}

#[async_trait]
impl Plugin for Ping {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("ping")
            .description("Responds to /ping")
            .version("0.1.0")
    }

    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.text().starts_with("/ping")
    }

    async fn handle(&self, ctx: Ctx) -> anyhow::Result<bool> {
        ctx.reply_text("pong").await?;
        Ok(false)
    }
}

#[async_trait]
impl Plugin for Hello {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("hello")
            .description("Greets when message matches regex")
            .version("0.1.0")
    }

    fn matches(&self, ctx: &Ctx) -> bool {
        HELLO_RE.is_match(&ctx.text())
    }

    async fn handle(&self, ctx: Ctx) -> anyhow::Result<bool> {
        ctx.reply_text(format!("Hello, {}!", ctx.nickname()))
            .await?;
        Ok(false)
    }
}

#[async_trait]
impl Plugin for Echo {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("echo")
            .description("Echoes messages starting with !")
            .version("0.1.0")
    }

    async fn handle(&self, ctx: Ctx) -> anyhow::Result<bool> {
        if let Some(content) = ctx.text().strip_prefix('!') {
            ctx.reply_text(format!("Echo: {}", content)).await?;
        }
        Ok(false)
    }
}

#[async_trait]
impl Plugin for PrivateHelp {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("private_help")
            .description("Help in private chat with /help")
            .version("0.1.0")
    }

    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.is_private() && ctx.text().starts_with("/help")
    }

    async fn handle(&self, ctx: Ctx) -> anyhow::Result<bool> {
        ctx.reply_text("这是私聊帮助").await?;
        Ok(false)
    }
}

#[async_trait]
impl Plugin for Admin {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("admin")
            .description("Admin command /admin, in group or user 12345")
            .version("0.1.0")
    }

    fn matches(&self, ctx: &Ctx) -> bool {
        ctx.text().starts_with("/admin") && (ctx.is_group() || ctx.user_id() == 12345)
    }

    async fn handle(&self, ctx: Ctx) -> anyhow::Result<bool> {
        ctx.reply_text("管理员命令").await?;
        Ok(true) // 阻止后续
    }
}
