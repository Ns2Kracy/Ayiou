use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    sync::Arc,
};

use anyhow::{Result, anyhow};

pub trait RuntimeService: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn version(&self) -> &'static str {
        "0.1.0"
    }
}

#[derive(Clone, Default)]
pub struct ServiceRegistry {
    services: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl ServiceRegistry {
    pub fn insert<S>(&mut self, service: S)
    where
        S: RuntimeService,
    {
        Arc::make_mut(&mut self.services).insert(TypeId::of::<S>(), Arc::new(service));
    }

    #[must_use]
    pub fn get<S>(&self) -> Option<Arc<S>>
    where
        S: RuntimeService,
    {
        self.services
            .get(&TypeId::of::<S>())
            .cloned()
            .and_then(|service| service.downcast::<S>().ok())
    }

    pub fn require<S>(&self) -> Result<Arc<S>>
    where
        S: RuntimeService,
    {
        self.get::<S>()
            .ok_or_else(|| anyhow!("runtime service `{}` is not registered", type_name::<S>()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CounterService {
        value: usize,
    }

    impl RuntimeService for CounterService {
        fn name(&self) -> &'static str {
            "counter"
        }
    }

    #[derive(Debug)]
    struct MissingService;

    impl RuntimeService for MissingService {
        fn name(&self) -> &'static str {
            "missing"
        }
    }

    #[test]
    fn registry_returns_typed_service_after_insert() {
        let mut registry = ServiceRegistry::default();

        registry.insert(CounterService { value: 42 });

        let service = registry
            .get::<CounterService>()
            .expect("counter service should be registered");
        assert_eq!(service.value, 42);
    }

    #[test]
    fn registry_returns_none_for_missing_service() {
        let registry = ServiceRegistry::default();

        assert!(registry.get::<MissingService>().is_none());
    }

    #[test]
    fn registry_require_reports_missing_service_type() {
        let registry = ServiceRegistry::default();

        let err = registry.require::<MissingService>().unwrap_err();

        assert!(
            err.to_string()
                .contains(std::any::type_name::<MissingService>())
        );
    }
}
