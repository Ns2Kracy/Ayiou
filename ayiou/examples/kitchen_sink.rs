use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
#[allow(unused_imports)]
use ayiou::command;
use ayiou::{
    Bot,
    core::{
        adapter::{Adapter, AdapterCapabilities, AdapterRuntime, MsgContext},
        model::{
            ChannelKind, ChannelRef, CommandInvocation, EventEnvelope, MessageEvent,
            OutboundMessage, OutboundReceipt, PlatformId, UserRef,
        },
        plugin_system::{
            ApplyConfigOutcome, Capability, ConfigUpdate, HandleOutcome, HandlerDecl, Permission,
            PluginHealth, PluginMetadata, RuntimePlugin, RuntimePluginManifest,
            RuntimePluginServices, negotiate_capabilities,
        },
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
    fn text(&self) -> String {
        self.envelope
            .message()
            .map(|message| message.text.clone())
            .unwrap_or_default()
    }

    fn user_id(&self) -> String {
        self.envelope
            .message()
            .map(|message| message.sender.user_id().to_string())
            .unwrap_or_default()
    }

    fn group_id(&self) -> Option<String> {
        self.envelope
            .message()
            .and_then(|message| match message.channel.kind() {
                ChannelKind::Group => Some(message.channel.channel_id().to_string()),
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
    #[command(name = "hello")]
    async fn execute(&self, ctx: &DemoCtx) -> Result<()> {
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
    #[command(name = "echo", alias = "say")]
    async fn echo(&self, _ctx: &DemoCtx, content: String) -> Result<()> {
        println!("echo: {content}");
        Ok(())
    }

    #[command(name = "add")]
    async fn add(&self, _ctx: &DemoCtx, left: i64, right: i64) -> Result<()> {
        println!("add: {}", left + right);
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct KitchenConfig {
    greeting: String,
}

struct KitchenPlugin {
    config: KitchenConfig,
    seen: Arc<Mutex<Vec<String>>>,
    services: Option<RuntimePluginServices<DemoCtx>>,
}

impl KitchenPlugin {
    fn new() -> Self {
        Self {
            config: KitchenConfig {
                greeting: "hello".to_string(),
            },
            seen: Arc::new(Mutex::new(Vec::new())),
            services: None,
        }
    }
}

#[async_trait]
impl RuntimePlugin<DemoCtx> for KitchenPlugin {
    fn kind(&self) -> &str {
        "kitchen"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("kitchen")
            .description(
                "manual RuntimePlugin covering manifest, handlers, config, capabilities, health",
            )
            .version("0.2.0")
    }

    fn manifest(&self) -> RuntimePluginManifest {
        RuntimePluginManifest::new(self.kind())
            .description("requires proactive send and can degrade without reactions")
            .version("0.2.0")
            .require_capability(Capability::ProactiveSend)
            .optional_capability(Capability::Reaction)
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![
            HandlerDecl::message_commands(["secure"], ["/"])
                .require_permission(Permission::user("admin"))
                .priority(5)
                .block(true),
            HandlerDecl::message_regex(["(?i)kitchen\\s+sink"])
                .require_permission(Permission::PlatformCapability(Capability::ProactiveSend))
                .priority(20),
        ]
    }

    async fn init(&mut self, services: RuntimePluginServices<DemoCtx>) -> Result<()> {
        let negotiation =
            negotiate_capabilities(&self.manifest(), &services.provided_capabilities());
        println!("capability negotiation: {negotiation:?}");
        self.services = Some(services);
        Ok(())
    }

    async fn start(&mut self, _services: RuntimePluginServices<DemoCtx>) -> Result<()> {
        self.apply_config(ConfigUpdate::dry_run(1, "bonjour"))
            .await?;
        self.apply_config(ConfigUpdate::new(2, "bonjour")).await?;
        Ok(())
    }

    async fn apply_config(&mut self, update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        if update.dry_run {
            println!("dry-run config v{}: {}", update.version, update.content);
            return Ok(ApplyConfigOutcome::skipped());
        }

        self.config.greeting = update.content;
        Ok(ApplyConfigOutcome::applied(update.version))
    }

    async fn handle_with_invocation(
        &self,
        ctx: &DemoCtx,
        invocation: Option<CommandInvocation>,
    ) -> Result<HandleOutcome> {
        if let Some(invocation) = invocation {
            self.seen
                .lock()
                .expect("seen lock")
                .push(format!("command:{}", invocation.command()));
            println!(
                "{} secure command args={}",
                self.config.greeting,
                invocation.args()
            );
            return Ok(HandleOutcome::block());
        }

        self.handle(ctx).await
    }

    async fn handle(&self, ctx: &DemoCtx) -> Result<HandleOutcome> {
        self.seen
            .lock()
            .expect("seen lock")
            .push(format!("regex:{}", ctx.text()));

        if let Some(message) = ctx.envelope.message() {
            let services = self
                .services
                .as_ref()
                .ok_or_else(|| anyhow!("plugin has not been initialized"))?;
            services
                .host
                .send_text(
                    message.channel.clone(),
                    format!("{} from kitchen", self.config.greeting),
                )
                .await?;
        }

        Ok(HandleOutcome::pass())
    }

    fn health(&self) -> PluginHealth {
        PluginHealth::healthy()
    }
}

#[derive(Clone)]
struct RecordingSender {
    messages: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ayiou::core::plugin_host::OutboundSender for RecordingSender {
    async fn send(&self, message: OutboundMessage) -> Result<OutboundReceipt> {
        self.messages
            .lock()
            .expect("messages lock")
            .push(message.plain_text());
        Ok(OutboundReceipt {
            message_id: Some("example-message".to_string()),
        })
    }
}

struct DemoAdapter {
    sender: Arc<RecordingSender>,
}

#[async_trait]
impl Adapter for DemoAdapter {
    type Ctx = DemoCtx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        self.start_with_runtime().await.events
    }

    async fn start_with_runtime(self) -> AdapterRuntime<Self::Ctx> {
        let (tx, rx) = mpsc::channel(8);
        let sender: Arc<dyn ayiou::core::plugin_host::OutboundSender> = self.sender.clone();

        let events = [
            DemoCtx::message("bot-a", "/secure admin open sesame", "admin", "group-a"),
            DemoCtx::message("bot-a", "please show the kitchen sink", "guest", "group-a"),
            DemoCtx::message("bot-a", "/say hello from macro", "guest", "group-a"),
            DemoCtx::message("bot-a", "/hello", "guest", "group-a"),
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
            sender: Some(sender),
        }
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            proactive_send: true,
            attachments: false,
            platform_extensions: Vec::new(),
        }
    }
}

async fn run_bot_demo() -> Result<()> {
    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sender = Arc::new(RecordingSender {
        messages: sent_messages.clone(),
    });

    Bot::new(DemoAdapter { sender })
        .with_plugin(HelloPlugin)
        .with_plugin(ToolsPlugin)
        .with_plugin_as("kitchen-main", KitchenPlugin::new())
        .workers(1)
        .queue_capacity(8)
        .command_prefixes(["/"])
        .run()
        .await;

    assert!(
        sent_messages
            .lock()
            .expect("messages lock")
            .iter()
            .any(|message| message.contains("bonjour"))
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    run_bot_demo().await?;
    println!("kitchen sink example completed");
    Ok(())
}
