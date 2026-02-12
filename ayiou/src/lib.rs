use std::{sync::Arc, time::Instant};

use log::{error, info};

use crate::core::{
    adapter::Adapter,
    observability::{MetricsSink, NoopMetrics, elapsed_ms},
    plugin::{DispatchOptions, Dispatcher, Plugin, PluginBox, PluginManager},
    scheduler::{Scheduler, TokioScheduler},
};

pub mod adapter;
pub mod core;
pub mod driver;
pub mod prelude;

pub use ayiou_macros::{Plugin, bot_plugin, command};

pub struct Bot<A: Adapter> {
    plugin_manager: PluginManager<A::Ctx>,
    dispatch_options: DispatchOptions,
    metrics_sink: Arc<dyn MetricsSink>,
    scheduler: Arc<dyn Scheduler>,
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

    pub fn scheduler(&self) -> Arc<dyn Scheduler> {
        self.scheduler.clone()
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
        pretty_env_logger::try_init().ok();
        info!("Starting Bot...");

        let mut event_rx = adapter.start().await;

        let plugins = self.plugin_manager.build();
        let dispatcher = Dispatcher::with_options(plugins, self.dispatch_options.clone());
        info!("Loaded {} plugins", self.plugin_manager.count());

        info!("Bot is running, press Ctrl+C to exit.");

        loop {
            tokio::select! {
                Some(ctx) = event_rx.recv() => {
                    self.metrics_sink.incr_counter("events_in_total", 1, &[]);
                    let dispatcher = dispatcher.clone();
                    let metrics = self.metrics_sink.clone();

                    tokio::spawn(async move {
                        let start = Instant::now();
                        if let Err(err) = dispatcher.dispatch(&ctx).await {
                            metrics.incr_counter("plugin_errors_total", 1, &[]);
                            error!("Plugin dispatch error: {}", err);
                        }
                        metrics.observe_duration_ms("plugin_handle_duration_ms", elapsed_ms(start), &[]);
                    });
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
