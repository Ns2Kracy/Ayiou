//! Global Driver for lifecycle hooks.
//!
//! This module provides a global Driver that allows registering lifecycle hooks
//! similar to NoneBot2's `driver.on_startup()` and `driver.on_shutdown()`.
//!
//! # Example
//!
//! ```ignore
//! use ayiou::get_driver;
//!
//! #[tokio::main]
//! async fn main() {
//!     {
//!         let mut driver = get_driver().write().await;
//!         driver.on_startup(|| async {
//!             println!("Starting up!");
//!         });
//!         driver.on_shutdown(|| async {
//!             println!("Shutting down!");
//!         });
//!     }
//!
//!     AyiouBot::new()
//!         .plugin::<MyPlugin>()
//!         .run("ws://...").await;
//! }
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use once_cell::sync::Lazy;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Boxed future type for lifecycle hooks
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Lifespan hook function type
type LifespanHook = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// Bot connection hook function type
type BotConnectHook = Arc<dyn Fn(i64) -> BoxFuture<'static, ()> + Send + Sync>;

/// Global Driver for lifecycle management
///
/// Provides hooks for application startup, shutdown, and bot connection events.
pub struct LifecycleDriver {
    startup_hooks: Vec<LifespanHook>,
    shutdown_hooks: Vec<LifespanHook>,
    bot_connect_hooks: Vec<BotConnectHook>,
}

impl Default for LifecycleDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleDriver {
    /// Create a new LifecycleDriver
    pub fn new() -> Self {
        Self {
            startup_hooks: Vec::new(),
            shutdown_hooks: Vec::new(),
            bot_connect_hooks: Vec::new(),
        }
    }

    /// Register a startup hook
    ///
    /// The function will be called when the application starts.
    pub fn on_startup<F, Fut>(&mut self, f: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.startup_hooks
            .push(Arc::new(move || Box::pin(f()) as BoxFuture<'static, ()>));
    }

    /// Register a shutdown hook
    ///
    /// The function will be called when the application shuts down.
    /// Shutdown hooks are called in reverse order of registration.
    pub fn on_shutdown<F, Fut>(&mut self, f: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.shutdown_hooks
            .push(Arc::new(move || Box::pin(f()) as BoxFuture<'static, ()>));
    }

    /// Register a bot connection hook
    ///
    /// The function will be called when a bot successfully connects.
    /// The `self_id` parameter is the bot's QQ number.
    pub fn on_bot_connect<F, Fut>(&mut self, f: F)
    where
        F: Fn(i64) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.bot_connect_hooks
            .push(Arc::new(move |id| Box::pin(f(id)) as BoxFuture<'static, ()>));
    }

    /// Run all startup hooks
    pub async fn run_startup_hooks(&self) {
        if self.startup_hooks.is_empty() {
            return;
        }

        info!("Running {} startup hooks", self.startup_hooks.len());
        for hook in &self.startup_hooks {
            debug!("Executing startup hook");
            hook().await;
        }
    }

    /// Run all shutdown hooks (in reverse order)
    pub async fn run_shutdown_hooks(&self) {
        if self.shutdown_hooks.is_empty() {
            return;
        }

        info!("Running {} shutdown hooks", self.shutdown_hooks.len());
        for hook in self.shutdown_hooks.iter().rev() {
            debug!("Executing shutdown hook");
            hook().await;
        }
    }

    /// Run all bot connection hooks
    pub async fn run_bot_connect_hooks(&self, self_id: i64) {
        if self.bot_connect_hooks.is_empty() {
            return;
        }

        info!(
            "Running {} bot connect hooks for bot {}",
            self.bot_connect_hooks.len(),
            self_id
        );
        for hook in &self.bot_connect_hooks {
            debug!("Executing bot connect hook");
            hook(self_id).await;
        }
    }

    /// Get the number of registered startup hooks
    pub fn startup_hook_count(&self) -> usize {
        self.startup_hooks.len()
    }

    /// Get the number of registered shutdown hooks
    pub fn shutdown_hook_count(&self) -> usize {
        self.shutdown_hooks.len()
    }

    /// Get the number of registered bot connect hooks
    pub fn bot_connect_hook_count(&self) -> usize {
        self.bot_connect_hooks.len()
    }
}

/// Global driver instance
static DRIVER: Lazy<RwLock<LifecycleDriver>> = Lazy::new(|| RwLock::new(LifecycleDriver::new()));

/// Get the global LifecycleDriver instance
///
/// # Example
///
/// ```ignore
/// use ayiou::get_driver;
///
/// async fn setup() {
///     let mut driver = get_driver().write().await;
///     driver.on_startup(|| async {
///         println!("Application starting!");
///     });
/// }
/// ```
pub fn get_driver() -> &'static RwLock<LifecycleDriver> {
    &DRIVER
}
