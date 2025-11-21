use crate::core::action::Bot;
use dashmap::DashMap;
use std::any::{Any, TypeId};
use std::sync::Arc;

/// The global context for the bot.
/// It acts as a dependency injection container and state manager.
#[derive(Clone, Default)]
pub struct Context {
    // Stores arbitrary data by TypeId. Thread-safe.
    pub storage: Arc<DashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    // Stores active bots by their self_id
    pub bots: Arc<DashMap<String, Arc<dyn Bot>>>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(DashMap::new()),
            bots: Arc::new(DashMap::new()),
        }
    }

    /// Register a bot instance
    pub fn register_bot(&self, bot: Arc<dyn Bot>) {
        self.bots.insert(bot.self_id().to_string(), bot);
    }

    /// Get a bot by ID
    pub fn get_bot(&self, id: &str) -> Option<Arc<dyn Bot>> {
        self.bots.get(id).map(|r| r.value().clone())
    }

    /// Get any bot (useful if there's only one)
    pub fn get_any_bot(&self) -> Option<Arc<dyn Bot>> {
        self.bots.iter().next().map(|r| r.value().clone())
    }

    /// Insert a dependency or state into the context.
    /// Recommend inserting Arc<T> for shared state.
    pub fn insert<T: 'static + Send + Sync>(&self, val: T) {
        self.storage.insert(TypeId::of::<T>(), Box::new(val));
    }

    /// Get a dependency from the context.
    /// The type T must be Clone. This encourages using Arc<T> for shared state,
    /// which is safe and efficient.
    pub fn get<T: 'static + Send + Sync + Clone>(&self) -> Option<T> {
        self.storage
            .get(&TypeId::of::<T>())
            .and_then(|r| r.downcast_ref::<T>().cloned())
    }
}
