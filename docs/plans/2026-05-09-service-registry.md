# Service Registry Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a typed runtime service registry so plugins can collaborate through explicit services instead of direct plugin references.

**Architecture:** Add `core::service` with `RuntimeService` and `ServiceRegistry`, store services by `TypeId`, and pass an `Arc<ServiceRegistry>` through `RuntimePluginServices`. Add `Bot::with_service(...)` so host code can register services before plugin initialization. The first version is host-provided services only.

**Tech Stack:** Rust 2024, `std::any::{Any, TypeId, type_name}`, `Arc`, existing `anyhow::Result`, existing async plugin runtime tests, TDD with `cargo test`.

---

### Task 1: Core Service Registry

**Files:**
- Create: `ayiou/src/core/service.rs`
- Modify: `ayiou/src/core.rs`

**Step 1: Write the failing test**

Create `ayiou/src/core/service.rs` with tests that describe the desired API before defining the API:

```rust
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
```

Add `pub mod service;` to `ayiou/src/core.rs`.

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p ayiou --no-default-features core::service
```

Expected: FAIL because `RuntimeService` and `ServiceRegistry` are not defined.

**Step 3: Write minimal implementation**

Add the service types above the tests:

```rust
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
```

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test -p ayiou --no-default-features core::service
```

Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/src/core.rs ayiou/src/core/service.rs
git commit -m "feat: add runtime service registry"
```

### Task 2: Expose Services Through RuntimePluginServices

**Files:**
- Modify: `ayiou/src/core/plugin_system.rs`

**Step 1: Write the failing test**

In `ayiou/src/core/plugin_system.rs` test module, add:

```rust
use crate::core::service::{RuntimeService, ServiceRegistry};

#[derive(Debug)]
struct TestCounterService {
    value: usize,
}

impl RuntimeService for TestCounterService {
    fn name(&self) -> &'static str {
        "test-counter"
    }
}

#[test]
fn runtime_plugin_services_provide_registered_services() {
    let host = PluginHost::<TestCtx>::new(None);
    let mut registry = ServiceRegistry::default();
    registry.insert(TestCounterService { value: 7 });
    let services = RuntimePluginServices::new(host).with_service_registry(registry);

    let service = services
        .require_service::<TestCounterService>()
        .expect("service should be available");

    assert_eq!(service.value, 7);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p ayiou --no-default-features runtime_plugin_services_provide_registered_services
```

Expected: FAIL because `RuntimePluginServices` has no registry or service lookup methods.

**Step 3: Write minimal implementation**

Update imports:

```rust
use crate::core::{
    command::parse_command_line,
    model::{BotId, CommandInvocation, PlatformId},
    plugin_host::PluginHost,
    plugin_runtime::{PluginLifecycleState, PluginRuntimeState},
    service::{RuntimeService, ServiceRegistry},
};
```

Add a field to `RuntimePluginServices<C>`:

```rust
pub service_registry: ServiceRegistry,
```

Update `Clone` and `new()`:

```rust
service_registry: self.service_registry.clone(),
```

and:

```rust
service_registry: ServiceRegistry::default(),
```

Add methods:

```rust
#[must_use]
pub fn with_service_registry(mut self, service_registry: ServiceRegistry) -> Self {
    self.service_registry = service_registry;
    self
}

#[must_use]
pub fn service<S>(&self) -> Option<Arc<S>>
where
    S: RuntimeService,
{
    self.service_registry.get::<S>()
}

pub fn require_service<S>(&self) -> Result<Arc<S>>
where
    S: RuntimeService,
{
    self.service_registry.require::<S>()
}
```

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test -p ayiou --no-default-features runtime_plugin_services_provide_registered_services
```

Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/src/core/plugin_system.rs
git commit -m "feat: expose services to runtime plugins"
```

### Task 3: Bot Builder Service Registration

**Files:**
- Modify: `ayiou/src/lib.rs`
- Modify: `ayiou/tests/plugin_host.rs`

**Step 1: Write the failing test**

In `ayiou/tests/plugin_host.rs`, add:

```rust
use ayiou::core::service::RuntimeService;

struct TestAclService {
    allowed_user: String,
}

impl RuntimeService for TestAclService {
    fn name(&self) -> &'static str {
        "test-acl"
    }
}

struct ServicePlugin {
    observed: Arc<std::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl RuntimePlugin<TestCtx> for ServicePlugin {
    fn kind(&self) -> &'static str {
        "service-plugin"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("service-plugin")
    }

    async fn init(&mut self, services: RuntimePluginServices<TestCtx>) -> Result<()> {
        let acl = services.require_service::<TestAclService>()?;
        self.observed.lock().unwrap().push(acl.allowed_user.clone());
        Ok(())
    }

    async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

#[tokio::test]
async fn bot_registered_services_are_available_during_plugin_init() {
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let bot = Bot::new(ClosedAdapter)
        .with_service(TestAclService {
            allowed_user: "admin".to_string(),
        })
        .with_plugin(ServicePlugin {
            observed: observed.clone(),
        });

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(*observed.lock().unwrap(), vec!["admin".to_string()]);
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p ayiou --no-default-features bot_registered_services_are_available_during_plugin_init
```

Expected: FAIL because `Bot::with_service` does not exist.

**Step 3: Write minimal implementation**

In `ayiou/src/lib.rs`, import the service types:

```rust
use crate::core::service::{RuntimeService, ServiceRegistry};
```

Add a field to `Bot<A>`:

```rust
service_registry: ServiceRegistry,
```

Initialize it in `Bot::new()`:

```rust
service_registry: ServiceRegistry::default(),
```

Add builder method:

```rust
#[must_use]
pub fn with_service<S>(mut self, service: S) -> Self
where
    S: RuntimeService,
{
    self.service_registry.insert(service);
    self
}
```

Pass services into plugin runtime services in `Bot::run()`:

```rust
RuntimePluginServices::new(plugin_host)
    .with_capabilities(adapter_capabilities_to_runtime(&adapter_capabilities))
    .with_service_registry(self.service_registry.clone())
```

**Step 4: Run test to verify it passes**

Run:

```bash
cargo test -p ayiou --no-default-features bot_registered_services_are_available_during_plugin_init
```

Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/src/lib.rs ayiou/tests/plugin_host.rs
git commit -m "feat: register runtime services on bot"
```

### Task 4: Documentation And Example

**Files:**
- Modify: `README.md`
- Modify: `docs/content/docs/getting-started.mdx`
- Modify: `docs/content/docs/kitchen-sink-example.mdx`
- Modify: `ayiou/examples/kitchen_sink.rs`

**Step 1: Write the failing example assertion**

In `ayiou/examples/kitchen_sink.rs`, add a small host-provided service:

```rust
struct GreetingService {
    greeting: String,
}

impl ayiou::core::service::RuntimeService for GreetingService {
    fn name(&self) -> &'static str {
        "greeting"
    }
}
```

Update one plugin `init()` to require it and store a value. Add an assertion in `run_bot_demo()` that proves the service value was used.

**Step 2: Run example test to verify it fails**

Run:

```bash
cargo test -p ayiou --no-default-features --example kitchen_sink
```

Expected: FAIL until `Bot::with_service(...)` is wired into the example.

**Step 3: Update docs**

Add a short Service Registry section:

````markdown
## Runtime Services

Plugins should not call other plugin instances directly. Register host-provided services on `Bot`, then request them from `RuntimePluginServices` during plugin initialization:

```rust
let bot = Bot::new(adapter)
    .with_service(AclService::new())
    .with_plugin(AdminPlugin);
```

```rust
async fn init(&mut self, services: RuntimePluginServices<Context>) -> anyhow::Result<()> {
    self.acl = Some(services.require_service::<AclService>()?);
    Ok(())
}
```
````

**Step 4: Run example and doc-adjacent checks**

Run:

```bash
cargo test -p ayiou --no-default-features --example kitchen_sink
cargo test -p ayiou --no-default-features
cargo test -p ayiou --all-features
```

Expected: PASS.

**Step 5: Commit**

```bash
git add README.md docs/content/docs/getting-started.mdx docs/content/docs/kitchen-sink-example.mdx ayiou/examples/kitchen_sink.rs
git commit -m "docs: document runtime services"
```

### Task 5: Final Verification

**Files:**
- No source edits unless verification reveals a bug.

**Step 1: Run full verification**

Run:

```bash
cargo fmt --all
cargo test -p ayiou --no-default-features
cargo test -p ayiou --all-features
cargo build -p ayiou --all-features --examples
git diff --check
```

Expected: all commands pass.

**Step 2: Inspect final state**

Run:

```bash
git status -sb
git log --oneline -5
```

Expected: clean working tree after commits; recent commits match the tasks above.
