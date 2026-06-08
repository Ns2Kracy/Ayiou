use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use ayiou::Bot;
use ayiou::core::adapter::{Adapter, MsgContext};
use ayiou::core::plugin_system::{
    HandleOutcome, HandlerDecl, PluginMetadata, RuntimePlugin, RuntimePluginManifest,
    RuntimePluginServices,
};
use ayiou::core::service::{RuntimeService, ServiceRegistry};
use ayiou::plugin;
use tokio::sync::mpsc;

static AUTO_HANDLES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
struct TestCtx {
    text: String,
}

impl TestCtx {
    fn empty() -> Self {
        Self {
            text: String::new(),
        }
    }

    fn text(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl MsgContext for TestCtx {
    fn text(&self) -> String {
        self.text.clone()
    }

    fn user_id(&self) -> String {
        "user".to_string()
    }

    fn group_id(&self) -> Option<String> {
        None
    }
}

struct ClosedAdapter;

#[async_trait]
impl Adapter for ClosedAdapter {
    type Ctx = TestCtx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }
}

struct OneEventAdapter;

#[async_trait]
impl Adapter for OneEventAdapter {
    type Ctx = TestCtx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let _ = tx.send(TestCtx::empty()).await;
        });
        rx
    }
}

struct AutoEventAdapter;

#[async_trait]
impl Adapter for AutoEventAdapter {
    type Ctx = TestCtx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let _ = tx.send(TestCtx::text("/auto")).await;
        });
        rx
    }
}

#[derive(Default)]
struct AutoDiscoveredPlugin;

#[plugin(name = "auto-discovered", prefix = "/", context = "TestCtx")]
impl AutoDiscoveredPlugin {
    async fn auto(&self, _ctx: &TestCtx) -> Result<()> {
        AUTO_HANDLES.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct StartPlugin {
    starts: Arc<AtomicUsize>,
}

#[async_trait]
impl RuntimePlugin<TestCtx> for StartPlugin {
    fn kind(&self) -> &'static str {
        "start-plugin"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("start-plugin")
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![HandlerDecl::wildcard_message()]
    }

    async fn start(&mut self, _services: RuntimePluginServices<TestCtx>) -> Result<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

struct HandlePlugin {
    handles: Arc<AtomicUsize>,
}

#[async_trait]
impl RuntimePlugin<TestCtx> for HandlePlugin {
    fn kind(&self) -> &'static str {
        "handle-plugin"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("handle-plugin")
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![HandlerDecl::wildcard_message()]
    }

    async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
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
impl RuntimePlugin<TestCtx> for ServicePlugin {
    fn kind(&self) -> &'static str {
        "service-plugin"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("service-plugin")
    }

    fn manifest(&self) -> RuntimePluginManifest {
        RuntimePluginManifest::new("service-plugin").require_service::<TestAclService>()
    }

    async fn init(&mut self, services: RuntimePluginServices<TestCtx>) -> Result<()> {
        let acl = services.require_service::<TestAclService>()?;
        self.observed.lock().unwrap().push(acl.allowed_user.clone());
        Ok(())
    }

    async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

struct ServiceProviderPlugin {
    allowed_user: String,
}

#[async_trait]
impl RuntimePlugin<TestCtx> for ServiceProviderPlugin {
    fn kind(&self) -> &'static str {
        "service-provider-plugin"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("service-provider-plugin")
    }

    fn register_services(&mut self, registry: &mut ServiceRegistry) -> Result<()> {
        registry.try_insert(TestAclService {
            allowed_user: self.allowed_user.clone(),
        })
    }

    async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
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
