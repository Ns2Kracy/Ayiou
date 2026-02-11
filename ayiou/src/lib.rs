use log::{error, info};

use crate::core::{
    adapter::Adapter,
    plugin::{DispatchOptions, Dispatcher, Plugin, PluginBox, PluginManager},
};

pub mod adapter;
pub mod core;
pub mod driver;
pub mod prelude;

pub use ayiou_macros::{Plugin, bot_plugin, command};

pub struct Bot<A: Adapter> {
    plugin_manager: PluginManager<A::Ctx>,
    dispatch_options: DispatchOptions,
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
        }
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
                    let dispatcher = dispatcher.clone();

                    tokio::spawn(async move {
                        if let Err(err) = dispatcher.dispatch(&ctx).await {
                            error!("Plugin dispatch error: {}", err);
                        }
                    });
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Bot is shutting down.");
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
