use ayiou::prelude::*;

#[derive(Plugin, Default)]
#[plugin(
    name = "echo",
    command = "echo",
    prefix = "/",
    context = "ayiou::adapter::console::ctx::Ctx",
    description = "Echo input text"
)]
struct EchoPlugin;

impl EchoPlugin {
    async fn execute(&self, ctx: &ConsoleCtx) -> anyhow::Result<()> {
        let args = ctx.command_args().unwrap_or_default();
        if args.is_empty() {
            ctx.reply_text("Usage: /echo <text>").await?;
            return Ok(());
        }
        ctx.reply_text(format!("echo: {}", args)).await?;
        Ok(())
    }
}

#[derive(Plugin, Default)]
#[plugin(
    name = "help",
    command = "help",
    prefix = "/",
    context = "ayiou::adapter::console::ctx::Ctx",
    description = "Show help"
)]
struct HelpPlugin;

impl HelpPlugin {
    async fn execute(&self, ctx: &ConsoleCtx) -> anyhow::Result<()> {
        ctx.reply_text("Commands: /help, /echo <text>, /ping, /add <a> <b>, /say <text>")
            .await?;
        Ok(())
    }
}

#[derive(Default)]
struct ToolboxPlugin;

#[bot_plugin(
    name = "toolbox",
    prefix = "/",
    context = "ayiou::adapter::console::ctx::Ctx",
    description = "bot_plugin macro demo"
)]
impl ToolboxPlugin {
    #[command]
    async fn ping(&self, ctx: &ConsoleCtx) -> anyhow::Result<()> {
        ctx.reply_text("pong").await
    }

    #[command]
    async fn add(&self, ctx: &ConsoleCtx, a: i64, b: i64) -> anyhow::Result<()> {
        ctx.reply_text(format!("{} + {} = {}", a, b, a + b)).await
    }

    #[command(name = "say")]
    async fn say_anything(&self, ctx: &ConsoleCtx, content: String) -> anyhow::Result<()> {
        if content.is_empty() {
            ctx.reply_text("Usage: /say <text>").await?;
            return Ok(());
        }
        ctx.reply_text(content).await
    }
}

#[tokio::main]
async fn main() {
    let bot = ConsoleBot::console()
        .register_plugin(HelpPlugin)
        .register_plugin(EchoPlugin)
        .register_plugin(ToolboxPlugin::default());

    println!("Console bot started. Type /help");
    bot.run_stdio().await;
}
