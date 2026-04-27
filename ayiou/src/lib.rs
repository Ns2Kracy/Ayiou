use std::{sync::Arc, time::Instant};

use log::{error, info};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::core::{
    adapter::Adapter,
    observability::{MetricsSink, NoopMetrics, elapsed_ms},
    plugin::{DispatchOptions, Plugin, PluginBox, PluginManager},
    plugin_host::PluginHost,
    plugin_runtime::PluginRuntimeState,
    plugin_system::{LegacyMessagePluginAdapter, RuntimePluginEngine, RuntimePluginServices},
    scheduler::{Scheduler, TokioScheduler},
    storage::{MemoryStore, Store},
};

pub mod adapter;
pub mod core;
pub mod driver;
pub mod prelude;

pub use ayiou_macros::{Plugin, bot_plugin, command};
pub use core::context::Context;
pub use core::model::*;
pub use core::runtime::{RuntimeController, RuntimeState};
pub use core::supervisor::*;

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
    plugin_manager: PluginManager<A::Ctx>,
    dispatch_options: DispatchOptions,
    metrics_sink: Arc<dyn MetricsSink>,
    scheduler: Arc<dyn Scheduler>,
    store: Arc<dyn Store>,
    runtime_options: BotRuntimeOptions,
}

struct BotRuntime<C> {
    engine: Arc<tokio::sync::RwLock<RuntimePluginEngine<C>>>,
    metrics_sink: Arc<dyn MetricsSink>,
    scheduler: Arc<dyn Scheduler>,
    options: BotRuntimeOptions,
}

impl<C> BotRuntime<C>
where
    C: crate::core::adapter::MsgContext + Send + 'static,
{
    fn new(
        engine: Arc<tokio::sync::RwLock<RuntimePluginEngine<C>>>,
        metrics_sink: Arc<dyn MetricsSink>,
        scheduler: Arc<dyn Scheduler>,
        options: BotRuntimeOptions,
    ) -> Self {
        Self {
            engine,
            metrics_sink,
            scheduler,
            options,
        }
    }

    async fn run(self, mut event_rx: mpsc::Receiver<C>) {
        let (work_tx, work_rx) = mpsc::channel::<C>(self.options.queue_capacity);
        let worker_rx = Arc::new(tokio::sync::Mutex::new(work_rx));
        let mut worker_handles = self.spawn_workers(worker_rx);

        info!("Bot is running, press Ctrl+C to exit.");

        loop {
            tokio::select! {
                maybe_ctx = event_rx.recv() => {
                    let Some(ctx) = maybe_ctx else {
                        info!("Adapter channel closed.");
                        break;
                    };
                    self.metrics_sink.incr_counter("events_in_total", 1, &[]);
                    match self.options.overflow_policy {
                        QueueOverflowPolicy::Backpressure => {
                            if work_tx.send(ctx).await.is_err() {
                                break;
                            }
                        }
                        QueueOverflowPolicy::DropNewest => {
                            if work_tx.try_send(ctx).is_err() {
                                self.metrics_sink.incr_counter("event_queue_rejected_total", 1, &[]);
                            }
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Bot is shutting down.");
                    if let Err(err) = self.scheduler.shutdown().await {
                        error!("Scheduler shutdown error: {}", err);
                    }
                    break;
                }
            }
        }

        drop(work_tx);
        for handle in worker_handles.drain(..) {
            handle.abort();
            let _ = handle.await;
        }
    }

    fn spawn_workers(
        &self,
        worker_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<C>>>,
    ) -> Vec<JoinHandle<()>> {
        (0..self.options.worker_count)
            .map(|_| {
                let engine = self.engine.clone();
                let metrics = self.metrics_sink.clone();
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

                        let start = Instant::now();
                        let dispatch_result = {
                            let engine = engine.read().await;
                            engine.handle_all(&ctx).await
                        };
                        if let Err(err) = dispatch_result {
                            metrics.incr_counter("plugin_errors_total", 1, &[]);
                            error!("Plugin dispatch error: {}", err);
                        }
                        metrics.observe_duration_ms(
                            "plugin_handle_duration_ms",
                            elapsed_ms(start),
                            &[],
                        );
                    }
                })
            })
            .collect()
    }
}

impl<A: Adapter> Default for Bot<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Adapter> Bot<A> {
    pub fn new() -> Self {
        Self {
            plugin_manager: PluginManager::new(),
            dispatch_options: DispatchOptions::default(),
            metrics_sink: Arc::new(NoopMetrics),
            scheduler: Arc::new(TokioScheduler::new()),
            store: Arc::new(MemoryStore::new()),
            runtime_options: BotRuntimeOptions::default(),
        }
    }

    pub fn with_metrics_sink(mut self, metrics_sink: Arc<dyn MetricsSink>) -> Self {
        self.metrics_sink = metrics_sink;
        self
    }

    pub fn with_scheduler(mut self, scheduler: Arc<dyn Scheduler>) -> Self {
        self.scheduler = scheduler;
        self
    }

    pub fn with_store(mut self, store: Arc<dyn Store>) -> Self {
        self.store = store;
        self
    }

    pub fn scheduler(&self) -> Arc<dyn Scheduler> {
        self.scheduler.clone()
    }

    pub fn store(&self) -> Arc<dyn Store> {
        self.store.clone()
    }

    pub fn workers(mut self, worker_count: usize) -> Self {
        self.runtime_options.worker_count = worker_count.max(1);
        self
    }

    pub fn queue_capacity(mut self, queue_capacity: usize) -> Self {
        self.runtime_options.queue_capacity = queue_capacity.max(1);
        self
    }

    pub fn queue_overflow_policy(mut self, overflow_policy: QueueOverflowPolicy) -> Self {
        self.runtime_options.overflow_policy = overflow_policy;
        self
    }

    pub fn command_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.dispatch_options = DispatchOptions::new([prefix]);
        self
    }

    pub fn command_prefixes(
        mut self,
        prefixes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.dispatch_options = DispatchOptions::new(prefixes);
        self
    }

    pub fn register_plugin<P: Plugin<A::Ctx>>(mut self, plugin: P) -> Self {
        self.plugin_manager.register(plugin);
        self
    }

    pub fn plugin<P: Plugin<A::Ctx> + Default>(mut self) -> Self {
        self.plugin_manager.register(P::default());
        self
    }

    pub fn command<C: Plugin<A::Ctx> + Default>(self) -> Self {
        self.plugin::<C>()
    }

    pub fn register_plugins(
        mut self,
        plugins: impl IntoIterator<Item = PluginBox<A::Ctx>>,
    ) -> Self {
        self.plugin_manager.register_all(plugins);
        self
    }

    pub fn plugin_manager(&self) -> &PluginManager<A::Ctx> {
        &self.plugin_manager
    }

    pub async fn run(mut self, adapter: A) {
        info!("Starting Bot...");

        let adapter_runtime = adapter.start_with_runtime().await;
        let runtime_state = PluginRuntimeState::default();
        let plugin_host = PluginHost::new(
            self.scheduler.clone(),
            self.store.clone(),
            adapter_runtime.sender.clone(),
        );
        let mut engine = RuntimePluginEngine::with_options(
            RuntimePluginServices::new(plugin_host),
            runtime_state.clone(),
            self.dispatch_options.clone(),
        );
        let plugins = self.plugin_manager.build();

        for plugin in plugins.iter() {
            engine.push(Box::new(LegacyMessagePluginAdapter::new(
                plugin.meta().name.clone(),
                plugin.clone(),
            )));
        }

        if let Err(err) = engine.init_all().await {
            error!("Plugin initialization error: {}", err);
            return;
        }

        if let Err(err) = engine.start_all().await {
            error!("Plugin startup error: {}", err);
            return;
        }

        info!("Loaded {} plugins", self.plugin_manager.count());
        let engine = Arc::new(tokio::sync::RwLock::new(engine));
        let runtime = BotRuntime::new(
            engine.clone(),
            self.metrics_sink.clone(),
            self.scheduler.clone(),
            self.runtime_options.clone(),
        );
        runtime.run(adapter_runtime.events).await;

        if let Err(err) = engine.write().await.stop_all().await {
            error!("Plugin shutdown error: {}", err);
        }
    }
}

pub type OneBotV11Bot = Bot<adapter::onebot::v11::adapter::OneBotV11Adapter>;
pub type ConsoleBot = Bot<adapter::console::adapter::ConsoleAdapter>;

impl ConsoleBot {
    pub fn console() -> Self {
        use crate::adapter::console::ext::ConsoleBotExt;
        Self::new().with_console_defaults()
    }

    pub async fn run_stdio(self) {
        use crate::adapter::console::ext::ConsoleBotExt;
        self.run_console().await;
    }
}
