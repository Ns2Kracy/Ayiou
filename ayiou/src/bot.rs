use crate::core::{
    adapter::Adapter, context::Context, driver::Driver, event::Event, plugin::Plugin,
};
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::{Notify, mpsc};
use tracing::{error, info};

pub struct AyiouBot {
    context: Context,
    plugins: Vec<Box<dyn Plugin>>,
    drivers: Vec<Arc<dyn Driver>>,
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
        context.insert(tx);
        Self {
            context,
            plugins: Vec::new(),
            drivers: Vec::new(),
            receiver: Some(rx),
        }
    }

    pub fn plugin<P: Plugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    pub fn plugins<P, I>(mut self, plugins: I) -> Self
    where
        P: Plugin + 'static,
        I: IntoIterator<Item = P>,
    {
        for plugin in plugins.into_iter() {
            self.plugins.push(Box::new(plugin));
        }
        self
    }

    pub fn driver<D>(mut self, driver: D) -> Self
    where
        D: Driver + 'static,
    {
        self.drivers.push(Arc::new(driver));
        self
    }

    pub async fn run(mut self) -> Result<()> {
        for plugin in &self.plugins {
            if let Err(e) = plugin.on_load(&self.context).await {
                error!("Failed to load plugin {}: {}", plugin.name(), e);
            }
        }

        let shutdown = Arc::new(Notify::new());

        let mut driver_handles = Vec::new();
        for driver in self.drivers {
            let mut adapter = driver
                .create_adapter(self.context.clone())
                .ok_or_else(|| anyhow!("Driver registered without an adapter"))?;

            adapter.set_driver(driver.clone());
            let adapter_arc: Arc<dyn Adapter> = Arc::from(adapter);

            self.context.register_adapter(adapter_arc.clone());

            let driver_clone = driver.clone();
            let shutdown_signal = shutdown.clone();
            let handle = tokio::spawn(async move {
                tokio::select! {
                    result = driver_clone.run(adapter_arc) => {
                        if let Err(e) = result {
                            error!("Driver failed: {}", e);
                        }
                    }
                    _ = shutdown_signal.notified() => {
                        info!("Driver shutdown signal received");
                    }
                }
            });
            driver_handles.push(handle);
        }

        let mut rx = self.receiver.take().expect("Receiver already taken");
        let ctx = self.context.clone();
        let plugins = Arc::new(self.plugins);

        info!("Event Loop Started");

        for plugin in plugins.iter() {
            if let Err(e) = plugin.on_start(&ctx).await {
                error!("Failed to start plugin {}: {}", plugin.name(), e);
            }
        }

        let event_loop_shutdown = shutdown.clone();
        let ctx_for_loop = ctx.clone();
        let plugins_for_loop = plugins.clone();
        let event_loop = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = event_loop_shutdown.notified() => {
                        info!("Event loop shutdown signal received");
                        break;
                    }
                    maybe_event = rx.recv() => {
                        match maybe_event {
                            Some(event) => {
                                let ctx_clone = ctx_for_loop.clone();
                                let plugins_clone = plugins_for_loop.clone();
                                let event_ref = event.clone();

                                tokio::spawn(async move {
                                    for plugin in plugins_clone.iter() {
                                        for handler in plugin.handlers() {
                                            handler.handle(ctx_clone.clone(), event_ref.clone()).await;
                                        }
                                    }
                                });
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        info!("Ayiou is running. Press Ctrl+C to exit.");
        tokio::signal::ctrl_c().await?;

        info!("Shutting down...");
        shutdown.notify_waiters();

        for handle in driver_handles {
            if let Err(e) = handle.await {
                error!("Driver task join error: {}", e);
            }
        }

        if let Err(e) = event_loop.await {
            error!("Event loop join error: {}", e);
        }

        for plugin in plugins.iter() {
            if let Err(e) = plugin.on_stop(&ctx).await {
                error!("Failed to stop plugin {}: {}", plugin.name(), e);
            }
        }

        info!("Shutdown complete.");

        Ok(())
    }
}
