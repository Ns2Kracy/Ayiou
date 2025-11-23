use crate::core::{
    adapter::Adapter, context::Context, driver::Driver, event::Event, plugin::Plugin,
};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct AyiouBot {
    context: Context,
    plugins: Vec<Box<dyn Plugin>>,
    adapters: HashMap<String, Box<dyn Adapter>>,
    driver_configs: Vec<(Arc<dyn Driver>, Option<String>)>, // Driver and optional Adapter ID
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
            adapters: HashMap::new(),
            driver_configs: Vec::new(),
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

    pub fn register_adapter<A, F>(mut self, builder: F) -> Self
    where
        A: Adapter + 'static + Clone,
        F: Fn(Context) -> A + Send + Sync + 'static,
    {
        let adapter = builder(self.context.clone());
        let id = adapter.name();
        self.adapters.insert(id.to_string(), Box::new(adapter));
        self
    }

    pub fn register_driver<D>(mut self, driver: D) -> Self
    where
        D: Driver + 'static,
    {
        self.driver_configs.push((Arc::new(driver), None));
        self
    }

    pub fn register_driver_with_adapter<D>(mut self, driver: D, adapter_id: &str) -> Self
    where
        D: Driver + 'static,
    {
        self.driver_configs
            .push((Arc::new(driver), Some(adapter_id.to_string())));
        self
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Ayiou Framework Starting...");

        for plugin in &self.plugins {
            if let Err(e) = plugin.on_load(&self.context).await {
                error!("Failed to load plugin {}: {}", plugin.name(), e);
            }
        }

        let single_adapter_id = if self.adapters.len() == 1 {
            self.adapters.keys().next().cloned()
        } else {
            None
        };

        let mut driver_handles = Vec::new();
        for (driver, adapter_id_option) in self.driver_configs {
            let adapter_id = match adapter_id_option {
                Some(id) => id,
                None => {
                    if let Some(ref id) = single_adapter_id {
                        id.clone()
                    } else {
                        return Err(anyhow!(
                            "Driver registered without an adapter ID, but there is not exactly one adapter registered."
                        ));
                    }
                }
            };

            let mut adapter = self
                .adapters
                .get(&adapter_id)
                .ok_or_else(|| anyhow!("Adapter '{}' not registered", adapter_id))?
                .clone();

            adapter.set_driver(driver.clone());
            let adapter_arc: Arc<dyn Adapter> = Arc::from(adapter);

            self.context.register_adapter(adapter_arc.clone());

            let handle = tokio::spawn(async move {
                if let Err(e) = driver.run(adapter_arc).await {
                    error!("Driver failed: {}", e);
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

        let event_loop = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let ctx_clone = ctx.clone();
                let plugins_clone = plugins.clone();
                let event_ref = event.clone();

                tokio::spawn(async move {
                    for plugin in plugins_clone.iter() {
                        for handler in plugin.handlers() {
                            handler.handle(ctx_clone.clone(), event_ref.clone()).await;
                        }
                    }
                });
            }
        });

        info!("Ayiou is running. Press Ctrl+C to exit.");
        tokio::signal::ctrl_c().await?;

        info!("Shutting down...");

        for handle in driver_handles {
            handle.abort();
        }

        event_loop.abort();

        Ok(())
    }
}
