use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use ayiou::core::adapter::{Adapter, AdapterRuntime};
use ayiou::core::model::{BotId, ChannelRef, EventEnvelope, MessageEvent, PlatformId, UserRef};
use ayiou::core::plugin::{
    ApplyConfigOutcome, CommandMeta, ConfigUpdate, ConversationKey, ConversationStore,
    HandleOutcome, HandlerDecl, MemoryConversationStore, Permission, PermissionDecision,
    PermissionService, PluginRuntimeState, RuntimePlugin, RuntimePluginEngine,
    RuntimePluginManifest, RuntimePluginServices,
};
use ayiou::core::service::{RuntimeService, ServiceRegistry};
use ayiou::plugin;
use ayiou::{Bot, Context};
use tokio::sync::mpsc;

static AUTO_HANDLES: AtomicUsize = AtomicUsize::new(0);

fn test_context(text: impl Into<String>) -> Context {
    let platform = PlatformId::new("test");
    let user = UserRef::new(platform.clone(), "user");
    let channel = ChannelRef::direct(platform.clone(), "user");
    let message = MessageEvent::new(user, channel, text.into());
    Context::new(
        EventEnvelope::new(BotId::new("test-bot"), platform).with_message(message),
        None,
        (),
    )
}

struct ClosedAdapter;

#[async_trait]
impl Adapter for ClosedAdapter {
    async fn start(self) -> AdapterRuntime {
        let (_tx, rx) = mpsc::channel(1);
        AdapterRuntime {
            events: rx,
            sender: None,
            capabilities: Vec::new(),
        }
    }
}

struct OneEventAdapter;

#[async_trait]
impl Adapter for OneEventAdapter {
    async fn start(self) -> AdapterRuntime {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let _ = tx.send(test_context("")).await;
        });
        AdapterRuntime {
            events: rx,
            sender: None,
            capabilities: Vec::new(),
        }
    }
}

struct AutoEventAdapter;

#[async_trait]
impl Adapter for AutoEventAdapter {
    async fn start(self) -> AdapterRuntime {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let _ = tx.send(test_context("/auto")).await;
        });
        AdapterRuntime {
            events: rx,
            sender: None,
            capabilities: Vec::new(),
        }
    }
}

#[derive(Default)]
struct AutoDiscoveredPlugin;

#[plugin(name = "auto-discovered", prefix = "/")]
impl AutoDiscoveredPlugin {
    async fn auto(&self, _ctx: &Context) -> Result<()> {
        AUTO_HANDLES.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct MetadataPlugin;

#[plugin(name = "metadata", prefix = "/", register = false)]
impl MetadataPlugin {
    #[command(
        name = "weather",
        alias = "forecast",
        aliases = ["wx"],
        summary = "Show a location forecast",
        usage = "/weather <city>",
        examples = ["/weather Taipei", "/forecast Tokyo"],
        priority = 10,
        block = false,
        permissions = ["weather.read"]
    )]
    async fn weather(&self, _ctx: &Context, _city: String) -> Result<()> {
        Ok(())
    }
}

struct AllowAdminService;

impl RuntimeService for AllowAdminService {
    fn name(&self) -> &'static str {
        "allow-admin"
    }
}

#[async_trait]
impl PermissionService for AllowAdminService {
    async fn check(&self, _ctx: &Context, permission: &Permission) -> Result<PermissionDecision> {
        Ok(match permission {
            Permission::Custom(name) if name == "admin" => PermissionDecision::Allow,
            other => PermissionDecision::Deny(format!("denied {other:?}")),
        })
    }
}

struct ChatbotServicePlugin {
    observed: Arc<std::sync::Mutex<Vec<String>>>,
    permission_service: std::sync::Mutex<Option<Arc<AllowAdminService>>>,
    conversation_store: std::sync::Mutex<Option<Arc<MemoryConversationStore>>>,
    instance_id: std::sync::Mutex<Option<String>>,
}

impl ChatbotServicePlugin {
    fn new(observed: Arc<std::sync::Mutex<Vec<String>>>) -> Self {
        Self {
            observed,
            permission_service: std::sync::Mutex::new(None),
            conversation_store: std::sync::Mutex::new(None),
            instance_id: std::sync::Mutex::new(None),
        }
    }
}

#[async_trait]
impl RuntimePlugin for ChatbotServicePlugin {
    fn kind(&self) -> &'static str {
        "chatbot-service-plugin"
    }

    async fn init(&mut self, services: RuntimePluginServices) -> Result<()> {
        *self.permission_service.lock().unwrap() =
            Some(services.require_permission_service::<AllowAdminService>()?);
        *self.conversation_store.lock().unwrap() =
            Some(services.require_conversation_store::<MemoryConversationStore>()?);
        *self.instance_id.lock().unwrap() = services.instance_id.clone();
        Ok(())
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![HandlerDecl::wildcard_message()]
    }

    async fn handle(&self, ctx: &Context) -> Result<HandleOutcome> {
        let permission_service = self
            .permission_service
            .lock()
            .unwrap()
            .clone()
            .expect("permission service initialized");
        let conversation_store = self
            .conversation_store
            .lock()
            .unwrap()
            .clone()
            .expect("conversation store initialized");
        let instance_id = self
            .instance_id
            .lock()
            .unwrap()
            .clone()
            .expect("instance id initialized");
        let key = ConversationKey::from_context(instance_id, ctx);

        let decision = permission_service
            .check(ctx, &Permission::custom("admin"))
            .await?;
        conversation_store
            .put(
                key.clone(),
                serde_json::json!({ "step": "awaiting-name" }),
                None,
            )
            .await?;
        let session = conversation_store
            .get(&key)
            .await?
            .expect("session state exists");

        self.observed.lock().unwrap().push(format!(
            "allowed={} step={}",
            decision.allowed(),
            session["step"].as_str().unwrap()
        ));
        Ok(HandleOutcome::pass())
    }
}

struct ConfigurablePlugin {
    default_city: Arc<std::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl RuntimePlugin for ConfigurablePlugin {
    fn kind(&self) -> &'static str {
        "configurable-plugin"
    }

    async fn apply_config(&mut self, update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        let city = update
            .values
            .get("default_city")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("default_city is required"))?;
        *self.default_city.lock().unwrap() = Some(city.to_string());
        Ok(ApplyConfigOutcome::applied(update.version))
    }

    async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

struct StartPlugin {
    starts: Arc<AtomicUsize>,
}

#[async_trait]
impl RuntimePlugin for StartPlugin {
    fn kind(&self) -> &'static str {
        "start-plugin"
    }
    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![HandlerDecl::wildcard_message()]
    }

    async fn start(&mut self, _services: RuntimePluginServices) -> Result<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

struct HandlePlugin {
    handles: Arc<AtomicUsize>,
}

#[async_trait]
impl RuntimePlugin for HandlePlugin {
    fn kind(&self) -> &'static str {
        "handle-plugin"
    }
    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![HandlerDecl::wildcard_message()]
    }

    async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
        self.handles.fetch_add(1, Ordering::SeqCst);
        Ok(HandleOutcome::pass())
    }
}

struct TestAclService {
    allowed_user: String,
}

impl RuntimeService for TestAclService {
    fn name(&self) -> &'static str {
        "test-acl"
    }
}

struct ServicePlugin {
    observed: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl RuntimePlugin for ServicePlugin {
    fn kind(&self) -> &'static str {
        "service-plugin"
    }
    fn manifest(&self) -> RuntimePluginManifest {
        RuntimePluginManifest::new("service-plugin").require_service::<TestAclService>()
    }

    async fn init(&mut self, services: RuntimePluginServices) -> Result<()> {
        let acl = services.require_service::<TestAclService>()?;
        self.observed.lock().unwrap().push(acl.allowed_user.clone());
        Ok(())
    }

    async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

struct ServiceProviderPlugin {
    allowed_user: String,
}

#[async_trait]
impl RuntimePlugin for ServiceProviderPlugin {
    fn kind(&self) -> &'static str {
        "service-provider-plugin"
    }
    fn register_services(&mut self, registry: &mut ServiceRegistry) -> Result<()> {
        registry.try_insert(TestAclService {
            allowed_user: self.allowed_user.clone(),
        })
    }

    async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

#[tokio::test]
async fn bot_invokes_plugin_start_once_before_exit() {
    let starts = Arc::new(AtomicUsize::new(0));
    let bot = Bot::new(ClosedAdapter).with_plugin(StartPlugin {
        starts: starts.clone(),
    });

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn bot_drains_queued_events_when_adapter_closes() {
    let handles = Arc::new(AtomicUsize::new(0));
    let bot = Bot::new(OneEventAdapter).with_plugin(HandlePlugin {
        handles: handles.clone(),
    });

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(handles.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn bot_automatically_loads_discovered_macro_plugins() {
    AUTO_HANDLES.store(0, Ordering::SeqCst);
    let bot = Bot::new(AutoEventAdapter);

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(AUTO_HANDLES.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn plugin_macro_declares_command_help_metadata() {
    let plugin = MetadataPlugin;

    let handlers = RuntimePlugin::declared_handlers(&plugin);
    assert_eq!(handlers.len(), 1);
    let handler = &handlers[0];

    assert_eq!(
        handler.commands,
        vec![
            "weather".to_string(),
            "forecast".to_string(),
            "wx".to_string()
        ]
    );
    assert_eq!(handler.command_prefixes, vec!["/".to_string()]);
    assert_eq!(handler.priority, 10);
    assert!(!handler.block);
    assert_eq!(
        handler.permissions,
        vec![Permission::custom("weather.read")]
    );
    assert_eq!(
        handler.command_meta,
        vec![
            CommandMeta::new("weather")
                .aliases(["forecast", "wx"])
                .summary("Show a location forecast")
                .usage("/weather <city>")
                .examples(["/weather Taipei", "/forecast Tokyo"])
        ]
    );
}

#[tokio::test]
async fn bot_registered_services_are_available_during_plugin_init() {
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let bot = Bot::new(ClosedAdapter)
        .with_service(TestAclService {
            allowed_user: "admin".to_string(),
        })
        .with_plugin(ServicePlugin {
            observed: observed.clone(),
        });

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(*observed.lock().unwrap(), vec!["admin".to_string()]);
}

#[tokio::test]
async fn bot_plugin_provided_services_are_available_during_plugin_init() {
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let bot = Bot::new(ClosedAdapter)
        .with_plugin(ServiceProviderPlugin {
            allowed_user: "plugin-admin".to_string(),
        })
        .with_plugin(ServicePlugin {
            observed: observed.clone(),
        });

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(*observed.lock().unwrap(), vec!["plugin-admin".to_string()]);
}

#[tokio::test]
async fn bot_exposes_permission_and_conversation_services_to_runtime_plugins() {
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let bot = Bot::new(OneEventAdapter)
        .with_service(AllowAdminService)
        .with_service(MemoryConversationStore::default())
        .with_plugin(ChatbotServicePlugin::new(observed.clone()));

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(
        *observed.lock().unwrap(),
        vec!["allowed=true step=awaiting-name".to_string()]
    );
}

#[tokio::test]
async fn runtime_applies_structured_plugin_config_values() {
    let default_city = Arc::new(std::sync::Mutex::new(None));
    let mut engine =
        RuntimePluginEngine::new(RuntimePluginServices::new(), PluginRuntimeState::default());
    engine.push(Box::new(ConfigurablePlugin {
        default_city: default_city.clone(),
    }));

    let outcome = engine
        .apply_config(
            "configurable-plugin",
            ConfigUpdate::new(7, serde_json::json!({ "default_city": "Taipei" })),
        )
        .await
        .expect("structured config should apply");

    assert_eq!(outcome, ApplyConfigOutcome::applied(7));
    assert_eq!(*default_city.lock().unwrap(), Some("Taipei".to_string()));
}
