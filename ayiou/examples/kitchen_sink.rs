use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use ayiou::prelude::*;
use tokio::sync::mpsc;

#[derive(Clone)]
struct DemoCtx {
    envelope: EventEnvelope,
    sender: Option<Arc<dyn ayiou::core::plugin_host::OutboundSender>>,
}

impl DemoCtx {
    fn message(bot_id: &str, text: impl Into<String>, user_id: &str, group_id: &str) -> Self {
        let platform = PlatformId::new("console");
        let user = UserRef::new(platform.clone(), user_id);
        let channel = ChannelRef::group(platform.clone(), group_id);
        let message = MessageEvent::new(user, channel, text.into());
        Self {
            envelope: EventEnvelope::new(bot_id, platform).with_message(message),
            sender: None,
        }
    }

    fn with_sender(mut self, sender: Arc<dyn ayiou::core::plugin_host::OutboundSender>) -> Self {
        self.sender = Some(sender);
        self
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

#[derive(Default, Plugin)]
#[plugin(
    name = "hello-derived",
    description = "single-command derive macro plugin",
    version = "0.2.0",
    command = "hello",
    prefix = "/",
    context = "DemoCtx"
)]
struct HelloDerivedPlugin;

impl HelloDerivedPlugin {
    async fn execute(&self, ctx: &DemoCtx) -> Result<()> {
        println!("derive macro handled: {}", ctx.text());
        Ok(())
    }
}

#[derive(Default)]
struct ToolsPlugin;

#[bot_plugin(
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
    instance_id: String,
    config: KitchenConfig,
    seen: Arc<Mutex<Vec<String>>>,
    services: Option<RuntimePluginServices<DemoCtx>>,
}

impl KitchenPlugin {
    fn new(instance_id: impl Into<String>) -> Self {
        Self {
            instance_id: instance_id.into(),
            config: KitchenConfig {
                greeting: "hello".to_string(),
            },
            seen: Arc::new(Mutex::new(Vec::new())),
            services: None,
        }
    }

    async fn demo_conversation(&self, ctx: &DemoCtx) -> Result<()> {
        let services = self
            .services
            .as_ref()
            .ok_or_else(|| anyhow!("plugin has not been initialized"))?;
        let store = services
            .sessions
            .as_ref()
            .ok_or_else(|| anyhow!("session store missing"))?;
        let key = SessionKey::new(
            services
                .bot_id
                .as_ref()
                .map(BotId::as_str)
                .unwrap_or("bot-a"),
            services
                .platform
                .as_ref()
                .map(PlatformId::as_str)
                .unwrap_or("console"),
            self.instance_id(),
            ctx.user_id(),
            ctx.group_id(),
        );

        let mut conversation = Conversation::resume(store.as_ref(), key.clone()).await?;
        let cursor = conversation
            .wait_next(
                store.as_ref(),
                serde_json::json!({ "question": "favorite feature?" }),
                None,
            )
            .await?;
        println!("conversation waiting at revision {}", cursor.revision());

        conversation
            .pause(store.as_ref(), "operator review", None)
            .await?;
        conversation
            .reject(store.as_ref(), "please answer with text", None)
            .await?;
        conversation.finish(store.as_ref()).await?;
        assert!(store.load(&key).await?.is_none());
        Ok(())
    }
}

#[async_trait]
impl RuntimePlugin<DemoCtx> for KitchenPlugin {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }

    fn kind(&self) -> &str {
        "kitchen"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new(self.instance_id.clone())
            .description(
                "manual RuntimePlugin covering manifest, handlers, config, session, health",
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

    async fn start(&mut self, services: RuntimePluginServices<DemoCtx>) -> Result<()> {
        services
            .host
            .store()
            .set_json("example:kitchen:start", &true)
            .await?;
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
            self.demo_conversation(ctx).await?;
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

struct ClosedAdapter;

#[async_trait]
impl Adapter for ClosedAdapter {
    type Ctx = DemoCtx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }
}

async fn run_engine_demo() -> Result<()> {
    let scheduler: Arc<dyn ayiou::core::scheduler::Scheduler> =
        Arc::new(ayiou::core::scheduler::TokioScheduler::new());
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let sessions: Arc<dyn SessionStore> = Arc::new(MemorySessionStore::new());
    let sent_messages = Arc::new(Mutex::new(Vec::new()));
    let sender = Arc::new(RecordingSender {
        messages: sent_messages.clone(),
    });

    let host = ayiou::core::plugin_host::PluginHost::new(
        scheduler.clone(),
        store.clone(),
        Some(sender.clone()),
    );
    let services = RuntimePluginServices::new(host)
        .with_identity("bot-a", "console")
        .with_sessions(sessions.clone())
        .with_metrics(Arc::new(NoopMetrics));
    let state = ayiou::core::plugin_runtime::PluginRuntimeState::default();
    let mut engine =
        RuntimePluginEngine::with_options(services, state.clone(), DispatchOptions::new(["/"]));

    engine.push(Box::new(HelloDerivedPlugin));
    engine.push(Box::new(ToolsPlugin));
    engine.push(Box::new(KitchenPlugin::new("kitchen-main")));

    engine.init_all().await?;
    engine.start_all().await?;
    engine
        .apply_config("kitchen-main", ConfigUpdate::dry_run(1, "bonjour"))
        .await?;
    engine
        .apply_config("kitchen-main", ConfigUpdate::new(2, "bonjour"))
        .await?;

    let admin_ctx = DemoCtx::message("bot-a", "/secure admin open sesame", "admin", "group-a")
        .with_sender(sender.clone());
    let blocked = engine.handle_all(&admin_ctx).await?;
    assert!(blocked);

    let regex_ctx = DemoCtx::message("bot-a", "please show the kitchen sink", "guest", "group-a")
        .with_sender(sender.clone());
    engine.handle_all(&regex_ctx).await?;

    let echo_ctx = DemoCtx::message("bot-a", "/say hello from macro", "guest", "group-a");
    engine.handle_all(&echo_ctx).await?;

    let derived_ctx = DemoCtx::message("bot-a", "/hello from derive", "guest", "group-a");
    engine.handle_all(&derived_ctx).await?;

    let snapshot = state.snapshot("kitchen-main");
    assert_eq!(snapshot.applied_config_version, 2);
    assert!(
        sent_messages
            .lock()
            .expect("messages lock")
            .iter()
            .any(|message| message.contains("bonjour"))
    );

    engine.stop_all().await?;
    Ok(())
}

async fn run_bot_builder_demo() {
    Bot::<ClosedAdapter>::new()
        .register_plugin(HelloDerivedPlugin)
        .register_plugin(ToolsPlugin)
        .workers(1)
        .queue_capacity(4)
        .run(ClosedAdapter)
        .await;
}

#[tokio::main]
async fn main() -> Result<()> {
    run_engine_demo().await?;
    run_bot_builder_demo().await;
    println!("kitchen sink example completed");
    Ok(())
}
