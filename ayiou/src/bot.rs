use crate::core::Driver;
use crate::core::event::Event;
use crate::core::plugin::Plugin;
use crate::core::{Adapter, context::Context};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct AyiouBot {
    context: Context,
    plugins: Vec<Box<dyn Plugin>>,
    adapter: Vec<Arc<dyn Adapter>>,
    driver: Vec<Arc<dyn Driver>>,
    sender: mpsc::Sender<Arc<dyn Event>>,
    receiver: Option<mpsc::Receiver<Arc<dyn Event>>>,
}

impl Default for AyiouBot {
    fn default() -> Self {
        Self::new()
    }
}

impl AyiouBot {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);
        let context = Context::new();

        // Inject the event sender into context so Adapters (and Plugins) can emit events
        context.insert(tx.clone());

        Self {
            context,
            plugins: Vec::new(),
            adapter: Vec::new(),
            driver: Vec::new(),
            sender: tx,
            receiver: Some(rx),
        }
    }

    pub fn register_adapter<A: Adapter + 'static>(mut self, adapter: A) -> Self {
        self.adapter.push(Arc::new(adapter));
        self
    }

    pub fn register_driver<D: Driver + 'static>(mut self, driver: D) -> Self {
        self.driver.push(Arc::new(driver));
        self
    }

    pub fn add_plugin<P: Plugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    pub fn context(&self) -> Context {
        self.context.clone()
    }

    /// Get a sender to emit events into the App loop (used by Adapters).
    pub fn get_sender(&self) -> mpsc::Sender<Arc<dyn Event>> {
        self.sender.clone()
    }

    pub async fn run(mut self) {
        info!("Ayiou Framework Starting...");

        // 1. Initialize Plugins
        for plugin in &self.plugins {
            info!("Loading plugin: {}", plugin.name());
            if let Err(e) = plugin.on_load(&self.context).await {
                error!("Failed to load plugin {}: {}", plugin.name(), e);
            }
        }

        let mut rx = self.receiver.take().expect("Receiver already taken");
        let ctx = self.context.clone();
        let plugins = Arc::new(self.plugins);

        info!("Event Loop Started");

        // 2. Start Plugins
        for plugin in plugins.iter() {
            if let Err(e) = plugin.on_start(&ctx).await {
                error!("Failed to start plugin {}: {}", plugin.name(), e);
            }
        }

        // 3. Event Loop
        while let Some(event) = rx.recv().await {
            let ctx_clone = ctx.clone();
            let plugins_clone = plugins.clone();
            let event_ref = event.clone();

            // Spawn a task for each event to ensure non-blocking handling
            tokio::spawn(async move {
                // Naive generic broadcasting to all plugins
                // In a real framework, you'd have a Matcher system here to filter events.
                for plugin in plugins_clone.iter() {
                    for handler in plugin.handlers() {
                        // Pass the Arc-ed event
                        handler.handle(ctx_clone.clone(), event_ref.clone()).await;
                    }
                }
            });
        }
    }
}
