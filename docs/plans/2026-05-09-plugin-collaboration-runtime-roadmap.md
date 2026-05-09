# Plugin Collaboration Runtime Roadmap Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.
>
> **Plan-only note:** This document records the next implementation sequence. Do not implement these tasks while writing or reviewing this plan.

**Goal:** Evolve plugin collaboration from host-only runtime services into an observable, lifecycle-aware collaboration layer without exposing direct plugin instance references.

**Architecture:** Keep the core rule: plugins collaborate through runtime-managed services and events, not through `get_plugin::<T>()`. Extend the current typed service registry in small vertical slices: first make services observable, then expose plugin/runtime inventory snapshots, then allow lifecycle-aware plugin-provided services, then add notification-style collaboration through a typed event bus.

**Tech Stack:** Rust 2024, existing `RuntimeService`, `ServiceRegistry`, `RuntimePluginManifest`, `RuntimePluginEngine`, `PluginRuntimeState`, `tokio`, `anyhow`, existing `cargo test` suite.

---

## Scope Principles

- Do not add direct plugin-to-plugin references.
- Do not introduce dynamic JSON RPC yet.
- Keep all new public APIs additive.
- Prefer typed Rust APIs first; string IDs are only for diagnostics and future control-plane display.
- Every task must start with a failing test and end with a verification command.

## Phase 1: Service Observability

### Task 1: Expose Service Descriptors

**Description:** Let runtime code and plugins inspect registered service metadata without downcasting service instances. This enables diagnostics, dependency reporting, and future control-plane views.

**Files:**
- Modify: `ayiou/src/core/service.rs`
- Modify: `ayiou/src/core/plugin_system.rs`
- Modify: `README.md`
- Modify: `docs/content/docs/getting-started.mdx`
- Modify: `docs/content/docs/kitchen-sink-example.mdx`

**Public API shape:**

```rust
pub struct ServiceDescriptor {
    pub key: ServiceKey,
    pub name: &'static str,
    pub version: &'static str,
}

impl ServiceRegistry {
    pub fn descriptor<S>(&self) -> Option<ServiceDescriptor>
    where
        S: RuntimeService;

    pub fn descriptor_for_key(&self, key: &ServiceKey) -> Option<ServiceDescriptor>;

    pub fn descriptors(&self) -> Vec<ServiceDescriptor>;
}

impl<C> RuntimePluginServices<C> {
    pub fn service_descriptor<S>(&self) -> Option<ServiceDescriptor>
    where
        S: RuntimeService;

    pub fn service_descriptors(&self) -> Vec<ServiceDescriptor>;
}
```

**Acceptance criteria:**
- [ ] Registry returns descriptors containing `ServiceKey`, `RuntimeService::name()`, and `RuntimeService::version()`.
- [ ] Re-registering the same service type updates both the instance and descriptor.
- [ ] `RuntimePluginServices` exposes descriptor helpers without exposing registry internals.
- [ ] Existing typed `get::<S>()` and `require::<S>()` behavior is unchanged.

**Verification:**
- [ ] `cargo test -p ayiou --no-default-features registry_`
- [ ] `cargo test -p ayiou --no-default-features runtime_plugin_services_describe_registered_services`
- [ ] `cargo test -p ayiou --no-default-features`
- [ ] `cargo test -p ayiou --all-features`

**Commit:** `feat: expose runtime service descriptors`

## Phase 2: Runtime Inventory

### Task 2: Add Plugin Runtime Snapshots

**Description:** Add a read-only inventory API that reports each registered plugin's instance ID, metadata, manifest, lifecycle state, config state, health, and dependency declarations. This is the foundation for a future web/control plane and for CLI diagnostics.

**Files:**
- Modify: `ayiou/src/core/plugin_system.rs`
- Modify: `ayiou/src/core/plugin_runtime.rs` only if existing state structs need a public aggregation type
- Modify: `docs/content/docs/v0-2-kernel-architecture.mdx`

**Public API shape:**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimePluginSnapshot {
    pub instance_id: String,
    pub kind: String,
    pub meta: PluginMetadata,
    pub manifest: RuntimePluginManifest,
    pub lifecycle: PluginInstanceState,
    pub health: PluginHealth,
}

impl<C> RuntimePluginEngine<C>
where
    C: Send + Sync + 'static,
{
    pub fn plugin_snapshots(&self) -> Vec<RuntimePluginSnapshot>;
}
```

**Acceptance criteria:**
- [ ] Snapshots include all registered plugins, sorted by `instance_id`.
- [ ] Snapshots include declared required/optional capabilities and services through `manifest`.
- [ ] Snapshots include current `PluginRuntimeState` and plugin `health()`.
- [ ] Calling `plugin_snapshots()` does not mutate lifecycle state or call plugin lifecycle hooks.

**Tests to add:**
- `runtime_plugin_engine_reports_plugin_snapshots`
- `plugin_snapshots_are_sorted_by_instance_id`
- `plugin_snapshots_do_not_trigger_lifecycle_hooks`

**Verification:**
- [ ] `cargo test -p ayiou --no-default-features plugin_snapshots`
- [ ] `cargo test -p ayiou --no-default-features`
- [ ] `cargo test -p ayiou --all-features`

**Commit:** `feat: expose plugin runtime snapshots`

## Phase 3: Structured Startup Diagnostics

### Task 3: Expose Startup Requirement Reports

**Description:** Convert startup preflight failures from ad hoc strings into a structured report that callers can inspect while preserving the current `anyhow::Error` path for `init_all()`.

**Files:**
- Modify: `ayiou/src/core/plugin_system.rs`
- Modify: `ayiou/src/lib.rs` only if `Bot::run()` should log structured summaries
- Modify: `docs/content/docs/v0-2-kernel-architecture.mdx`

**Public API shape:**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRequirementFailure {
    pub instance_id: String,
    pub missing_capabilities: Vec<Capability>,
    pub missing_services: Vec<ServiceKey>,
}

impl<C> RuntimePluginEngine<C>
where
    C: Send + Sync + 'static,
{
    pub fn validate_startup_requirements(&self) -> Vec<PluginRequirementFailure>;
}
```

**Acceptance criteria:**
- [ ] `validate_startup_requirements()` returns every plugin with missing required capabilities or services.
- [ ] `init_all()` uses the same validation path and does not partially initialize plugins when failures exist.
- [ ] Runtime state records each failing plugin's own message.
- [ ] Existing error strings remain readable and include instance IDs plus missing items.

**Tests to add:**
- `runtime_plugin_engine_reports_all_startup_requirement_failures`
- `runtime_plugin_engine_does_not_init_any_plugin_when_preflight_fails`
- `startup_requirement_validation_is_read_only`

**Verification:**
- [ ] `cargo test -p ayiou --no-default-features startup_requirement`
- [ ] `cargo test -p ayiou --no-default-features required`
- [ ] `cargo test -p ayiou --all-features`

**Commit:** `feat: expose plugin startup requirement reports`

## Phase 4: Plugin-Provided Services

### Task 4: Add Lifecycle-Aware Service Registration Hook

**Description:** Allow plugins to provide services through the runtime registry without exposing plugin instances. Service registration happens before startup requirement preflight, so consumer plugins can declare required services and still fail before any `init()` side effects.

**Files:**
- Modify: `ayiou/src/core/service.rs`
- Modify: `ayiou/src/core/plugin_system.rs`
- Modify: `ayiou/tests/plugin_host.rs`
- Modify: `docs/content/docs/kitchen-sink-example.mdx`

**Public API shape:**

```rust
impl ServiceRegistry {
    pub fn try_insert<S>(&mut self, service: S) -> anyhow::Result<()>
    where
        S: RuntimeService;
}

#[async_trait]
pub trait RuntimePlugin<C: Sync + 'static>: Send + Sync + 'static {
    fn register_services(&mut self, _registry: &mut ServiceRegistry) -> anyhow::Result<()> {
        Ok(())
    }
}
```

**Design rules:**
- `Bot::with_service(...)` keeps current replace semantics for host setup.
- Plugin-provided services use `try_insert` to reject duplicate service types by default.
- The runtime calls every plugin's `register_services()` before `validate_startup_requirements()`.
- A service registration failure aborts startup before any plugin `init()` runs.
- Services should be lightweight facades over shared state, not references to plugin trait objects.

**Acceptance criteria:**
- [ ] A provider plugin can register a typed service consumed by another plugin.
- [ ] A consumer plugin can declare `.require_service::<ProviderService>()` and initialize successfully.
- [ ] Duplicate plugin-provided service types fail startup before `init()`.
- [ ] Service registration failure records the provider plugin as failed.
- [ ] No API returns a direct plugin instance reference.

**Tests to add:**
- `plugin_provided_service_satisfies_consumer_manifest`
- `plugin_service_registration_runs_before_dependency_preflight`
- `duplicate_plugin_provided_service_fails_startup`
- `plugin_service_registration_failure_prevents_partial_init`

**Verification:**
- [ ] `cargo test -p ayiou --no-default-features plugin_provided_service`
- [ ] `cargo test -p ayiou --no-default-features startup_requirement`
- [ ] `cargo test -p ayiou --all-features`
- [ ] `cargo build -p ayiou --all-features --examples`

**Commit:** `feat: support plugin-provided runtime services`

## Phase 5: Notification-Style Collaboration

### Task 5: Add Typed Runtime Event Bus Service

**Description:** Add a built-in service for notification-style plugin collaboration where request/response service calls are too coupled. The event bus should be typed, local-process, and bounded.

**Files:**
- Create: `ayiou/src/core/event_bus.rs`
- Modify: `ayiou/src/core.rs`
- Modify: `ayiou/src/lib.rs`
- Modify: `ayiou/examples/kitchen_sink.rs`
- Modify: `docs/content/docs/kitchen-sink-example.mdx`

**Public API shape:**

```rust
pub trait RuntimeEvent: Clone + Send + Sync + 'static {
    fn topic() -> &'static str;
}

pub struct RuntimeEventBus { ... }

impl RuntimeEventBus {
    pub fn new(capacity: usize) -> Self;
    pub fn publish<E>(&self, event: E) -> anyhow::Result<usize>
    where
        E: RuntimeEvent;
    pub fn subscribe<E>(&self) -> tokio::sync::broadcast::Receiver<E>
    where
        E: RuntimeEvent;
}
```

**Design rules:**
- The bus is a `RuntimeService`, so plugins obtain it through `RuntimePluginServices`.
- It is local to one `Bot` runtime.
- It must use bounded channels and return subscriber lag/closed errors clearly.
- Do not add cross-process delivery or persistence in this task.

**Acceptance criteria:**
- [ ] A plugin can publish a typed event.
- [ ] Another plugin can subscribe during `init()` or `start()`.
- [ ] Slow subscribers get explicit lag errors from Tokio broadcast semantics.
- [ ] The event bus is opt-in through `Bot::with_service(RuntimeEventBus::new(...))` or a small builder helper.

**Tests to add:**
- `event_bus_delivers_typed_events_to_subscribers`
- `event_bus_is_scoped_to_one_runtime`
- `event_bus_reports_lag_for_slow_subscribers`

**Verification:**
- [ ] `cargo test -p ayiou --no-default-features event_bus`
- [ ] `cargo test -p ayiou --no-default-features`
- [ ] `cargo test -p ayiou --all-features`
- [ ] `cargo build -p ayiou --all-features --examples`

**Commit:** `feat: add runtime event bus service`

## Phase 6: Service Health And Readiness

### Task 6: Add Runtime Service Health Reporting

**Description:** Let services report health/readiness separately from plugin health so operators can distinguish plugin code failures from shared service degradation.

**Files:**
- Modify: `ayiou/src/core/service.rs`
- Modify: `ayiou/src/core/plugin_system.rs`
- Modify: `docs/content/docs/v0-2-kernel-architecture.mdx`

**Public API shape:**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceHealth {
    pub healthy: bool,
    pub ready: bool,
    pub detail: Option<String>,
}

pub trait RuntimeService: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str { "0.1.0" }
    fn health(&self) -> ServiceHealth { ServiceHealth::healthy() }
}
```

**Acceptance criteria:**
- [ ] Existing services compile without adding `health()` manually.
- [ ] Service descriptors or a separate snapshot API exposes service health.
- [ ] Unhealthy optional services do not automatically block startup.
- [ ] Unready required services fail startup only if a clear readiness policy is included in this task; otherwise keep readiness informational.

**Tests to add:**
- `runtime_service_health_defaults_to_healthy_ready`
- `service_registry_reports_service_health`
- `required_service_readiness_policy_is_explicit`

**Verification:**
- [ ] `cargo test -p ayiou --no-default-features service_health`
- [ ] `cargo test -p ayiou --no-default-features`
- [ ] `cargo test -p ayiou --all-features`

**Commit:** `feat: report runtime service health`

## Checkpoints

### Checkpoint A: After Phases 1-3

- [ ] Services and plugins are observable through stable snapshot APIs.
- [ ] Startup requirement failures are structured and non-partial.
- [ ] Docs explain diagnostics and startup preflight behavior.
- [ ] Verification passes:

```bash
cargo fmt --all
cargo test -p ayiou --no-default-features
cargo test -p ayiou --all-features
cargo build -p ayiou --all-features --examples
git diff --check
```

### Checkpoint B: After Phases 4-6

- [ ] Plugins can provide services without direct plugin references.
- [ ] Plugins can communicate through either typed services or typed events.
- [ ] Service and plugin health are separately observable.
- [ ] Verification passes:

```bash
cargo fmt --all
cargo test -p ayiou --no-default-features
cargo test -p ayiou --all-features
cargo build -p ayiou --all-features --examples
git diff --check
```

## Risks And Mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Plugin-provided services introduce startup cycles | High | Register all services before dependency preflight; reject direct plugin references; document services as facades |
| Dynamic event bus becomes hidden RPC | Medium | Keep bus notification-only; no request/response protocol in the first event-bus task |
| Service metadata becomes a compatibility promise too early | Medium | Expose type name, service name, and version only; avoid semantic version negotiation until a separate design |
| Control-plane needs richer data later | Low | Add snapshot structs now; extend them additively |

## Open Questions For Future Design

- Should plugin-provided service duplicates always fail, or should host services override plugin services?
- Should required service readiness block startup, or should it only affect health reporting?
- Should event bus subscriptions be declared in manifest, or kept as runtime behavior?
- Should service versions eventually support compatibility ranges?
