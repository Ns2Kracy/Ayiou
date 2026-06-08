use std::{collections::HashSet, sync::Arc};

use log::{error, info};
use tokio::{sync::mpsc, task::JoinHandle};

#[cfg(feature = "control-plane")]
use crate::control_plane::{self, ControlPlaneOptions};
#[cfg(feature = "control-plane")]
use crate::core::control::RuntimeControlHandle;
use crate::core::{
    adapter::Adapter,
    plugin_host::PluginHost,
    plugin_runtime::PluginRuntimeState,
    plugin_system::{
        DispatchOptions, RegisteredPlugin, RuntimePlugin, RuntimePluginEngine,
        RuntimePluginServices, discovered_plugins,
    },
    service::{RuntimeService, ServiceRegistry},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QueueOverflowPolicy {
    #[default]
    Backpressure,
    DropNewest,
}

#[derive(Clone, Debug)]
pub struct BotRuntimeOptions {
    pub worker_count: usize,
    pub queue_capacity: usize,
    pub overflow_policy: QueueOverflowPolicy,
}

impl Default for BotRuntimeOptions {
    fn default() -> Self {
        Self {
            worker_count: 4,
            queue_capacity: 256,
            overflow_policy: QueueOverflowPolicy::Backpressure,
        }
    }
}

pub struct Bot<A: Adapter> {
    adapter: A,
    plugins: Vec<RegisteredPlugin<A::Ctx>>,
    service_registry: ServiceRegistry,
    dispatch_options: DispatchOptions,
    runtime_options: BotRuntimeOptions,
    #[cfg(feature = "control-plane")]
    control_plane_options: Option<ControlPlaneOptions>,
}

struct BotRuntime<C> {
    engine: Arc<tokio::sync::RwLock<RuntimePluginEngine<C>>>,
    options: BotRuntimeOptions,
}

impl<C> BotRuntime<C>
where
    C: crate::core::adapter::MsgContext + Send + 'static,
{
    const fn new(
        engine: Arc<tokio::sync::RwLock<RuntimePluginEngine<C>>>,
        options: BotRuntimeOptions,
    ) -> Self {
        Self { engine, options }
    }

    async fn run(self, mut event_rx: mpsc::Receiver<C>) {
        let (work_tx, work_rx) = mpsc::channel::<C>(self.options.queue_capacity);
        let worker_rx = Arc::new(tokio::sync::Mutex::new(work_rx));
        let worker_handles = self.spawn_workers(&worker_rx);
        let mut drain_workers = true;

        info!("Bot is running, press Ctrl+C to exit.");

        loop {
            tokio::select! {
                maybe_ctx = event_rx.recv() => {
                    let Some(ctx) = maybe_ctx else {
                        info!("Adapter channel closed.");
                        break;
                    };
                    match self.options.overflow_policy {
                        QueueOverflowPolicy::Backpressure => {
                            if work_tx.send(ctx).await.is_err() {
                                break;
                            }
                        }
                        QueueOverflowPolicy::DropNewest => {
                            match work_tx.try_send(ctx) {
                                Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                                Err(mpsc::error::TrySendError::Closed(_)) => break,
                            }
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Bot is shutting down.");
                    drain_workers = false;
                    break;
                }
            }
        }

        drop(work_tx);
        for handle in worker_handles {
            if !drain_workers {
                handle.abort();
            }
            let _ = handle.await;
        }
    }

    fn spawn_workers(
        &self,
        worker_rx: &Arc<tokio::sync::Mutex<mpsc::Receiver<C>>>,
    ) -> Vec<JoinHandle<()>> {
        (0..self.options.worker_count)
            .map(|_| {
                let engine = self.engine.clone();
                let worker_rx = worker_rx.clone();
                tokio::spawn(async move {
                    loop {
                        let maybe_ctx = {
                            let mut rx = worker_rx.lock().await;
                            rx.recv().await
                        };

                        let Some(ctx) = maybe_ctx else {
                            break;
                        };

                        let dispatch_result = {
                            let engine = engine.read().await;
                            engine.handle_all(&ctx).await
                        };
                        if let Err(err) = dispatch_result {
                            error!("Plugin dispatch error: {err}");
                        }
                    }
                })
            })
            .collect()
    }
}

impl<A: Adapter> Bot<A> {
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            plugins: Vec::new(),
            service_registry: ServiceRegistry::default(),
            dispatch_options: DispatchOptions::default(),
            runtime_options: BotRuntimeOptions::default(),
            #[cfg(feature = "control-plane")]
            control_plane_options: None,
        }
    }

    #[must_use]
    pub fn workers(mut self, worker_count: usize) -> Self {
        self.runtime_options.worker_count = worker_count.max(1);
        self
    }

    #[must_use]
    pub fn queue_capacity(mut self, queue_capacity: usize) -> Self {
        self.runtime_options.queue_capacity = queue_capacity.max(1);
        self
    }

    #[must_use]
    pub const fn queue_overflow_policy(mut self, overflow_policy: QueueOverflowPolicy) -> Self {
        self.runtime_options.overflow_policy = overflow_policy;
        self
    }

    #[must_use]
    pub fn command_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.dispatch_options = DispatchOptions::new([prefix]);
        self
    }

    #[must_use]
    pub fn command_prefixes(
        mut self,
        prefixes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.dispatch_options = DispatchOptions::new(prefixes);
        self
    }

    #[must_use]
    pub fn with_service<S>(mut self, service: S) -> Self
    where
        S: RuntimeService,
    {
        self.service_registry.insert(service);
        self
    }

    #[must_use]
    pub fn with_plugin<P: RuntimePlugin<A::Ctx>>(mut self, plugin: P) -> Self {
        self.plugins
            .push(RegisteredPlugin::from_plugin(Box::new(plugin)));
        self
    }

    #[must_use]
    pub fn with_plugin_as<P: RuntimePlugin<A::Ctx>>(
        mut self,
        instance_id: impl Into<String>,
        plugin: P,
    ) -> Self {
        self.plugins
            .push(RegisteredPlugin::new(instance_id, Box::new(plugin)));
        self
    }

    #[cfg(feature = "control-plane")]
    #[must_use]
    pub fn control_plane(mut self, options: ControlPlaneOptions) -> Self {
        self.control_plane_options = Some(options);
        self
    }

    pub async fn run(mut self) {
        info!("Starting Bot...");
        #[cfg(feature = "control-plane")]
        let control_plane_options = self.control_plane_options.clone();

        self.load_discovered_plugins();

        let adapter_capabilities = self.adapter.capabilities();
        let adapter_runtime = self.adapter.start_with_runtime().await;
        let runtime_state = PluginRuntimeState::default();
        let plugin_host = PluginHost::new(adapter_runtime.sender.clone());
        let mut engine = RuntimePluginEngine::with_options(
            RuntimePluginServices::new(plugin_host)
                .with_capabilities(adapter_capabilities_to_runtime(&adapter_capabilities))
                .with_service_registry(self.service_registry.clone()),
            runtime_state.clone(),
            self.dispatch_options.clone(),
        );
        for registered in self.plugins.drain(..) {
            let (instance_id, plugin) = registered.into_parts();
            engine.push_as(instance_id, plugin);
        }

        if let Err(err) = engine.init_all().await {
            error!("Plugin initialization error: {err}");
            return;
        }

        if let Err(err) = engine.start_all().await {
            error!("Plugin startup error: {err}");
            return;
        }

        info!("Loaded {} plugins", engine.plugins().len());
        let engine = Arc::new(tokio::sync::RwLock::new(engine));
        #[cfg(feature = "control-plane")]
        let control = RuntimeControlHandle::new(engine.clone());
        #[cfg(feature = "control-plane")]
        if let Some(options) = control_plane_options {
            if let Err(err) = control_plane::spawn(options, control.clone()) {
                error!("Control plane configuration error: {err}");
                return;
            }
        }
        let runtime = BotRuntime::new(engine.clone(), self.runtime_options.clone());
        runtime.run(adapter_runtime.events).await;

        if let Err(err) = engine.write().await.stop_all().await {
            error!("Plugin shutdown error: {err}");
        }
    }

    fn load_discovered_plugins(&mut self)
    where
        A::Ctx: Send + Sync + 'static,
    {
        let mut explicit_ids: HashSet<String> = self
            .plugins
            .iter()
            .map(|plugin| plugin.instance_id().to_string())
            .collect();

        for plugin in discovered_plugins::<A::Ctx>() {
            if explicit_ids.insert(plugin.instance_id().to_string()) {
                self.plugins.push(plugin);
            }
        }
    }
}

fn adapter_capabilities_to_runtime(
    capabilities: &crate::core::adapter::AdapterCapabilities,
) -> Vec<crate::core::plugin_system::Capability> {
    let mut out = Vec::new();

    if capabilities.proactive_send {
        out.push(crate::core::plugin_system::Capability::ProactiveSend);
    }

    if capabilities.attachments {
        out.push(crate::core::plugin_system::Capability::RichSegments);
    }

    out.extend(
        capabilities
            .platform_extensions
            .iter()
            .cloned()
            .map(crate::core::plugin_system::Capability::custom),
    );

    out
}

#[cfg(feature = "adapter-onebot-v11")]
pub type OneBotV11Bot = Bot<crate::adapter::onebot::v11::adapter::OneBotV11Adapter>;

#[cfg(feature = "adapter-console")]
pub type ConsoleBot = Bot<crate::adapter::console::adapter::ConsoleAdapter>;

#[cfg(feature = "adapter-console")]
impl ConsoleBot {
    #[must_use]
    pub fn console() -> Self {
        Self::new(crate::adapter::console::adapter::ConsoleAdapter::new())
            .command_prefixes(["/", "!", "."])
    }
}

#[cfg(feature = "adapter-onebot-v11")]
impl OneBotV11Bot {
    pub fn ws(url: impl Into<String> + Send) -> Self {
        Self::new(crate::adapter::onebot::v11::adapter::OneBotV11Adapter::new(
            url,
        ))
        .command_prefixes(["/", "!", "."])
    }

    pub fn ws_with_token(url: impl Into<String> + Send, token: impl Into<String> + Send) -> Self {
        Self::new(crate::adapter::onebot::v11::adapter::OneBotV11Adapter::with_token(url, token))
            .command_prefixes(["/", "!", "."])
    }
}
