use std::borrow::Cow;

use anyhow::Result;
use async_trait::async_trait;
#[cfg(feature = "control-plane")]
use ayiou::ControlPlaneOptions;
use ayiou::{
    Bot,
    core::{
        adapter::{Adapter, MsgContext},
        model::{ChannelKind, ChannelRef, EventEnvelope, MessageEvent, PlatformId, UserRef},
    },
    plugin,
};
use tokio::sync::mpsc;

#[derive(Clone)]
struct DemoCtx {
    envelope: EventEnvelope,
}

impl DemoCtx {
    fn message(bot_id: &str, text: impl Into<String>, user_id: &str, group_id: &str) -> Self {
        let platform = PlatformId::new("console");
        let user = UserRef::new(platform.clone(), user_id);
        let channel = ChannelRef::group(platform.clone(), group_id);
        let message = MessageEvent::new(user, channel, text.into());
        Self {
            envelope: EventEnvelope::new(bot_id, platform).with_message(message),
        }
    }
}

impl MsgContext for DemoCtx {
    fn text(&self) -> Cow<'_, str> {
        self.envelope
            .message()
            .map(|message| Cow::Borrowed(message.text.as_str()))
            .unwrap_or_else(|| Cow::Borrowed(""))
    }

    fn user_id(&self) -> Cow<'_, str> {
        self.envelope
            .message()
            .map(|message| Cow::Borrowed(message.sender.user_id()))
            .unwrap_or_else(|| Cow::Borrowed(""))
    }

    fn group_id(&self) -> Option<Cow<'_, str>> {
        self.envelope
            .message()
            .and_then(|message| match message.channel.kind() {
                ChannelKind::Group => Some(Cow::Borrowed(message.channel.channel_id())),
                ChannelKind::Direct | ChannelKind::Channel => None,
            })
    }
}

#[derive(Default)]
struct HelloPlugin;

#[plugin(
    name = "hello",
    description = "single-command plugin macro",
    version = "0.2.0",
    prefix = "/",
    context = "DemoCtx"
)]
impl HelloPlugin {
    async fn hello(&self, ctx: &DemoCtx) -> Result<()> {
        println!("plugin macro handled: {}", ctx.text());
        Ok(())
    }
}

#[derive(Default)]
struct ToolsPlugin;

#[plugin(
    name = "tools",
    description = "multi-command attribute macro plugin",
    prefix = "/",
    context = "DemoCtx"
)]
impl ToolsPlugin {
    async fn echo(&self, _ctx: &DemoCtx, content: String) -> Result<()> {
        println!("echo: {content}");
        Ok(())
    }

    async fn add(&self, _ctx: &DemoCtx, left: i64, right: i64) -> Result<()> {
        println!("add: {}", left + right);
        Ok(())
    }
}

struct DemoAdapter;

#[async_trait]
impl Adapter for DemoAdapter {
    type Ctx = DemoCtx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        let (tx, rx) = mpsc::channel(8);

        let events = [
            DemoCtx::message("bot-a", "/echo hello from macro", "guest", "group-a"),
            DemoCtx::message("bot-a", "/add 20 22", "guest", "group-a"),
            DemoCtx::message("bot-a", "/hello", "guest", "group-a"),
        ];

        tokio::spawn(async move {
            for event in events {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });

        rx
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
