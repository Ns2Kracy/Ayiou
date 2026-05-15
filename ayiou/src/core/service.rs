use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    sync::Arc,
};

use anyhow::{Result, anyhow};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceHealth {
    pub healthy: bool,
    pub ready: bool,
    pub detail: Option<String>,
}

impl ServiceHealth {
    #[must_use]
    pub const fn healthy() -> Self {
        Self {
            healthy: true,
            ready: true,
            detail: None,
        }
    }
}

pub trait RuntimeService: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn health(&self) -> ServiceHealth {
        ServiceHealth::healthy()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ServiceKey {
    type_id: TypeId,
    type_name: &'static str,
}

impl ServiceKey {
    #[must_use]
    pub fn of<S>() -> Self
    where
        S: RuntimeService,
    {
        Self {
            type_id: TypeId::of::<S>(),
            type_name: type_name::<S>(),
        }
    }

    #[must_use]
    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    #[must_use]
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDescriptor {
    pub key: ServiceKey,
    pub name: &'static str,
    pub version: &'static str,
}

impl ServiceDescriptor {
    #[must_use]
    pub fn of<S>(service: &S) -> Self
    where
        S: RuntimeService,
    {
        Self {
            key: ServiceKey::of::<S>(),
            name: service.name(),
            version: service.version(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceSnapshot {
    pub descriptor: ServiceDescriptor,
    pub health: ServiceHealth,
}

#[derive(Clone)]
struct RegisteredService {
    descriptor: ServiceDescriptor,
    health: Arc<dyn Fn() -> ServiceHealth + Send + Sync>,
    service: Arc<dyn Any + Send + Sync>,
}

#[derive(Clone, Default)]
pub struct ServiceRegistry {
    services: Arc<HashMap<TypeId, RegisteredService>>,
}

impl ServiceRegistry {
    pub fn insert<S>(&mut self, service: S)
    where
        S: RuntimeService,
    {
        let service = Arc::new(service);
        let health_service = service.clone();
        Arc::make_mut(&mut self.services).insert(
            TypeId::of::<S>(),
            RegisteredService {
                descriptor: ServiceDescriptor::of(service.as_ref()),
                health: Arc::new(move || health_service.health()),
                service,
            },
        );
    }

    pub fn try_insert<S>(&mut self, service: S) -> Result<()>
    where
        S: RuntimeService,
    {
        if self.services.contains_key(&TypeId::of::<S>()) {
            return Err(anyhow!(
                "runtime service `{}` is already registered",
                type_name::<S>()
            ));
        }
        self.insert(service);
        Ok(())
    }

    #[must_use]
    pub fn contains_key(&self, key: &ServiceKey) -> bool {
        self.services.contains_key(&key.type_id)
    }

    #[must_use]
    pub fn descriptor<S>(&self) -> Option<ServiceDescriptor>
    where
        S: RuntimeService,
    {
        self.descriptor_for_key(&ServiceKey::of::<S>())
    }

    #[must_use]
    pub fn descriptor_for_key(&self, key: &ServiceKey) -> Option<ServiceDescriptor> {
        self.services
            .get(&key.type_id)
            .map(|registered| registered.descriptor.clone())
    }

    #[must_use]
    pub fn descriptors(&self) -> Vec<ServiceDescriptor> {
        let mut descriptors: Vec<_> = self
            .services
            .values()
            .map(|registered| registered.descriptor.clone())
            .collect();
        descriptors.sort_by(|left, right| left.key.type_name.cmp(right.key.type_name));
        descriptors
    }

    #[must_use]
    pub fn snapshot<S>(&self) -> Option<ServiceSnapshot>
    where
        S: RuntimeService,
    {
        self.snapshot_for_key(&ServiceKey::of::<S>())
    }

    #[must_use]
    pub fn snapshot_for_key(&self, key: &ServiceKey) -> Option<ServiceSnapshot> {
        self.services
            .get(&key.type_id)
            .map(|registered| ServiceSnapshot {
                descriptor: registered.descriptor.clone(),
                health: (registered.health)(),
            })
    }

    #[must_use]
    pub fn snapshots(&self) -> Vec<ServiceSnapshot> {
        let mut snapshots: Vec<_> = self
            .services
            .values()
            .map(|registered| ServiceSnapshot {
                descriptor: registered.descriptor.clone(),
                health: (registered.health)(),
            })
            .collect();
        snapshots.sort_by(|left, right| {
            left.descriptor
                .key
                .type_name
                .cmp(right.descriptor.key.type_name)
        });
        snapshots
    }

    #[must_use]
    pub fn get<S>(&self) -> Option<Arc<S>>
    where
        S: RuntimeService,
    {
        self.services
            .get(&TypeId::of::<S>())
            .map(|registered| registered.service.clone())
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

    struct VersionedCounterService {
        value: usize,
        version: &'static str,
    }

    impl RuntimeService for VersionedCounterService {
        fn name(&self) -> &'static str {
            "versioned-counter"
        }

        fn version(&self) -> &'static str {
            self.version
        }
    }

    struct UnreadyService;

    impl RuntimeService for UnreadyService {
        fn name(&self) -> &'static str {
            "unready"
        }

        fn health(&self) -> ServiceHealth {
            ServiceHealth {
                healthy: true,
                ready: false,
                detail: Some("warming up".to_string()),
            }
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

    #[test]
    fn service_key_captures_type_metadata() {
        let key = ServiceKey::of::<CounterService>();

        assert_eq!(key.type_id(), TypeId::of::<CounterService>());
        assert_eq!(key.type_name(), std::any::type_name::<CounterService>());
    }

    #[test]
    fn registry_describes_registered_services() {
        let mut registry = ServiceRegistry::default();

        registry.insert(CounterService { value: 42 });

        let descriptor = registry
            .descriptor::<CounterService>()
            .expect("counter service should have a descriptor");
        assert_eq!(descriptor.key, ServiceKey::of::<CounterService>());
        assert_eq!(descriptor.name, "counter");
        assert_eq!(descriptor.version, "0.1.0");
        assert_eq!(registry.descriptors(), vec![descriptor]);
    }

    #[test]
    fn registry_replaces_service_descriptor_with_instance() {
        let mut registry = ServiceRegistry::default();

        registry.insert(VersionedCounterService {
            value: 1,
            version: "1.0.0",
        });
        registry.insert(VersionedCounterService {
            value: 2,
            version: "2.0.0",
        });

        let service = registry
            .require::<VersionedCounterService>()
            .expect("versioned counter should be registered");
        let descriptor = registry
            .descriptor::<VersionedCounterService>()
            .expect("versioned counter should have a descriptor");

        assert_eq!(service.value, 2);
        assert_eq!(descriptor.version, "2.0.0");
        assert_eq!(registry.descriptors(), vec![descriptor]);
    }

    #[test]
    fn runtime_service_health_defaults_to_healthy_ready() {
        let service = CounterService { value: 42 };

        assert_eq!(service.health(), ServiceHealth::healthy());
    }

    #[test]
    fn service_registry_reports_service_health() {
        let mut registry = ServiceRegistry::default();

        registry.insert(UnreadyService);

        let snapshot = registry
            .snapshot::<UnreadyService>()
            .expect("unready service should have a snapshot");

        assert_eq!(snapshot.descriptor.name, "unready");
        assert_eq!(
            snapshot.health,
            ServiceHealth {
                healthy: true,
                ready: false,
                detail: Some("warming up".to_string()),
            }
        );
        assert_eq!(registry.snapshots(), vec![snapshot]);
    }
}
