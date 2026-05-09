# Service Dependency Manifest Design

## Context

Ayiou now has a typed runtime service registry. Plugins can request host-provided services during `init()` and `start()`, but the dependency is only visible inside plugin code. That makes startup failures later than necessary and prevents tooling from inspecting plugin requirements.

Direct plugin-to-plugin calls remain out of scope. Plugin collaboration should continue to flow through runtime-managed services.

## Decision

Add typed service dependency declarations to `RuntimePluginManifest`:

```rust
RuntimePluginManifest::new("admin")
    .require_service::<AclService>()
    .optional_service::<ProfileService>()
```

The manifest stores a lightweight `ServiceKey` for each dependency. The key includes the Rust `TypeId` for exact runtime lookup and the type name for diagnostics.

`RuntimePluginEngine::init_all()` checks required service dependencies before calling `RuntimePlugin::init()`. If a required service is missing, startup fails with a message that names the plugin instance and missing service types. Optional services are exposed in the manifest for future tooling but do not affect startup.

## Alternatives Considered

### Keep Lookup Only In `init()`

Plugins can already call `services.require_service::<T>()`. This is simple, but hides dependencies from the runtime until plugin initialization code runs. It also makes future plugin management and diagnostics weaker.

### Dynamic String Service IDs

Services could be declared by names such as `"acl@1"`. This is flexible across dynamic boundaries, but it gives up compile-time type guidance in the first-class Rust API and introduces version matching before Ayiou needs it.

### Plugin Instance References

The runtime could expose `get_plugin::<T>()`. This was rejected because it couples plugins to concrete instances, makes reload and ordering harder, and bypasses runtime governance.

## Error Handling

Missing required services use one startup failure path, parallel to missing required capabilities:

```text
plugin `<instance_id>` missing required services: [<type names>]
```

The runtime records the error in `PluginRuntimeState` and returns it from `init_all()`.

## Consequences

- Plugin dependencies become explicit and inspectable.
- Existing plugins keep working because service declarations are additive.
- Optional service declarations are available for docs/tooling without forcing a negotiation policy yet.
- The first implementation remains host-provided-service only; plugin-provided service lifecycle stays a separate design.
