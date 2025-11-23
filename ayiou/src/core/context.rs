use crate::core::adapter::Adapter;
use dashmap::DashMap;
use std::any::{Any, TypeId};
use std::sync::Arc;

/// The global context for the bot.
/// It acts as a dependency injection container and state manager.
#[derive(Clone, Default)]
pub struct Context {
    // Stores arbitrary data by TypeId. Thread-safe.
    pub storage: Arc<DashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    // Stores active adapters by their name
    pub adapters: Arc<DashMap<String, Arc<dyn Adapter>>>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(DashMap::new()),
            adapters: Arc::new(DashMap::new()),
        }
    }

    /// Register a adapter instance
    pub fn register_adapter(&self, adapter: Arc<dyn Adapter>) {
        self.adapters.insert(adapter.name().to_string(), adapter);
    }

    /// Get a adapter by ID
    pub fn get_adapter(&self, name: &str) -> Option<Arc<dyn Adapter>> {
        self.adapters.get(name).map(|r| r.value().clone())
    }

    /// Get any adapter (useful if there's only one)
    pub fn get_any_adapter(&self) -> Option<Arc<dyn Adapter>> {
        self.adapters.iter().next().map(|r| r.value().clone())
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
