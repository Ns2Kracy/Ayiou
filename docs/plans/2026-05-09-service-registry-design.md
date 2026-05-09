# Service Registry Design

## Context

Ayiou plugins need a controlled way to collaborate. Direct plugin-to-plugin calls would couple plugins to concrete instances, create startup ordering problems, make reload and isolation difficult, and bypass runtime governance.

The approved direction is to model plugin collaboration as runtime-managed services:

- plugins do not receive references to other plugin instances
- plugins request typed services from `RuntimePluginServices`
- services are registered in a central `ServiceRegistry`
- missing services are handled at startup or at retrieval time

## Goals

- Keep plugin collaboration explicit and type-safe.
- Allow plugins and host code to provide reusable services.
- Avoid hidden dependency graphs between plugin instances.
- Support future required/optional service negotiation without committing to a large RPC system.

## Non-Goals

- No dynamic JSON RPC in the first version.
- No cross-process service calls.
- No service hot replacement.
- No service permission DSL.
- No direct `get_plugin::<T>()` API.
- No synchronous initialization dependency resolution.

## Public Model

Introduce a `RuntimeService` marker trait:

```rust
pub trait RuntimeService: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn version(&self) -> &'static str {
        "0.1.0"
    }
}
```

Introduce a `ServiceRegistry` that stores services by `TypeId` and exposes typed lookup:

```rust
pub struct ServiceRegistry { ... }

impl ServiceRegistry {
    pub fn insert<S>(&mut self, service: S)
    where
        S: RuntimeService;

    pub fn get<S>(&self) -> Option<Arc<S>>
    where
        S: RuntimeService;

    pub fn require<S>(&self) -> Result<Arc<S>>
    where
        S: RuntimeService;
}
```

Extend `RuntimePluginServices<C>` with:

```rust
impl<C> RuntimePluginServices<C> {
    pub fn service<S>(&self) -> Option<Arc<S>>
    where
        S: RuntimeService;

    pub fn require_service<S>(&self) -> Result<Arc<S>>
    where
        S: RuntimeService;
}
```

## Registration

The first version should support host-side service registration through the bot builder:

```rust
Bot::new(adapter)
    .with_service(AclService::new(...))
    .with_plugin(AdminPlugin)
```

Plugin-provided services can be added later after the lifecycle rules are explicit. The safer first step is host-provided services only, because they are available before plugin initialization and avoid plugin startup dependency cycles.

## Required Services

The first implementation can make service lookup explicit in `init()`:

```rust
async fn init(&mut self, services: RuntimePluginServices<Context>) -> Result<()> {
    self.acl = Some(services.require_service::<AclService>()?);
    Ok(())
}
```

Later, `RuntimePluginManifest` can gain required and optional service declarations:

```rust
RuntimePluginManifest::new("admin")
    .require_service::<AclService>()
    .optional_service::<UserProfileService>()
```

That manifest-level check should not be part of the first implementation unless the typed registry is already stable.

## Data Flow

1. Host registers services on `Bot`.
2. `Bot::run()` builds a shared `ServiceRegistry`.
3. `RuntimePluginServices` carries the registry to each plugin during `init()` and `start()`.
4. Plugins store typed `Arc<S>` handles if they need repeated service calls.
5. Service call errors stay in the service API and propagate through normal `Result` returns.

## Error Handling

Use a structured missing-service error message that includes the Rust type name:

```text
runtime service `<type>` is not registered
```

Do not hide service call failures. If a plugin requires a service in `init()`, startup should fail in the same way missing capabilities currently fail.

## Testing

Minimum tests:

- registry returns a typed service after insertion
- registry returns `None` for a missing service
- `RuntimePluginServices::require_service` reports a useful error
- a plugin can read a registered service during `init()`
- `Bot::with_service(...)` makes the service available to plugin initialization

Use a small `CounterService` or `AclService` in tests. Avoid mocks unless they reduce setup complexity.

## Future Extensions

- plugin-provided services with lifecycle-aware registration
- manifest-level required/optional service negotiation
- service version compatibility checks
- service permissions and call auditing
- event bus for notification-style collaboration
- cross-process service adapter if Ayiou later grows a control plane
