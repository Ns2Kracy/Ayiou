use anyhow::Result;
use async_trait::async_trait;
#[cfg(feature = "control-plane")]
use ayiou::ControlPlaneOptions;
use ayiou::{
    Bot, Context,
    core::{
        adapter::{Adapter, AdapterRuntime},
        model::{BotId, ChannelRef, EventEnvelope, MessageEvent, PlatformId, UserRef},
    },
    plugin,
};
use tokio::sync::mpsc;

fn demo_context(bot_id: &str, text: impl Into<String>, user_id: &str, group_id: &str) -> Context {
    let platform = PlatformId::new("console");
    let user = UserRef::new(platform.clone(), user_id);
    let channel = ChannelRef::group(platform.clone(), group_id);
    let message = MessageEvent::new(user, channel, text.into());
    Context::new(
        EventEnvelope::new(BotId::new(bot_id), platform).with_message(message),
        None,
        (),
    )
}

#[derive(Default)]
struct HelloPlugin;

#[plugin(
    name = "hello",
    description = "single-command plugin macro",
    version = "0.2.0",
    prefix = "/"
)]
impl HelloPlugin {
    async fn hello(&self, ctx: &Context) -> Result<()> {
        println!("plugin macro handled: {}", ctx.text());
        Ok(())
    }
}

#[derive(Default)]
struct ToolsPlugin;

#[plugin(
    name = "tools",
    description = "multi-command attribute macro plugin",
    prefix = "/"
)]
impl ToolsPlugin {
    async fn echo(&self, _ctx: &Context, content: String) -> Result<()> {
        println!("echo: {content}");
        Ok(())
    }

    async fn add(&self, _ctx: &Context, left: i64, right: i64) -> Result<()> {
        println!("add: {}", left + right);
        Ok(())
    }
}

struct DemoAdapter;

#[async_trait]
impl Adapter for DemoAdapter {
    async fn start(self) -> AdapterRuntime {
        let (tx, rx) = mpsc::channel(8);

        let events = [
            demo_context("bot-a", "/echo hello from macro", "guest", "group-a"),
            demo_context("bot-a", "/add 20 22", "guest", "group-a"),
            demo_context("bot-a", "/hello", "guest", "group-a"),
        ];

        tokio::spawn(async move {
            for event in events {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });

        AdapterRuntime {
            events: rx,
            sender: None,
            capabilities: Vec::new(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let bot = Bot::new(DemoAdapter)
        .workers(1)
        .queue_capacity(8)
        .command_prefixes(["/"]);

    #[cfg(feature = "control-plane")]
    let bot = {
        let bind = "127.0.0.1:32187";
        let token = "kitchen-sink-token";
        println!("control plane: http://{bind}/ token={token}");
        bot.control_plane(ControlPlaneOptions::new().bind(bind).token(token))
    };

    bot.run().await;

    println!("kitchen sink example completed");
    Ok(())
}
