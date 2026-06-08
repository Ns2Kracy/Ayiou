# Control Plane and Hot Reload Design

## Goal

Add an Ayiou-managed control plane for plugin inspection, lifecycle control, and hot reload, with an embedded React/Tailwind management UI. The design must preserve the current single `RuntimePlugin` model and keep plugin authors on one consistent development path.

## Non-Goals

- Do not introduce a second plugin model for Rhai or WASM.
- Do not make compiled Rust inventory plugins code-reloadable; they can be controlled, but their code is already linked into the process.
- Do not expose direct plugin-to-plugin access or raw service registry access through the control plane.
- Do not start a public unauthenticated admin server.

## Top-Level Architecture

`RuntimePlugin` remains the only plugin model inside the engine. Control plane operations act through a runtime control handle that owns the safe operations for inspecting and mutating plugin lifecycle state.

```text
Bot
  -> RuntimePluginEngine
  -> RuntimeControlHandle
  -> optional ControlPlaneServer
  -> optional embedded Web UI
```

Rhai and WASM should later enter the system as reloadable backends that produce `RuntimePlugin` instances. The control plane should not care whether a plugin was compiled Rust, Rhai, or WASM; it should only observe a plugin descriptor that says whether reload is supported.

## Control Plane Configuration

Control plane startup is explicit. The host bind address is optional, but the token is required.

```rust
Bot::new(adapter)
    .control_plane(
        ControlPlaneOptions::new()
            .bind("127.0.0.1:32187")
            .token("change-me"),
    )
    .run()
    .await;
```

Defaults:

- If `bind` is omitted, use `127.0.0.1:32187`.
- If `token` is omitted, the control plane does not start and reports a clear configuration error.
- The token is user-controlled. Builder configuration is the first API; environment/config-file wiring can be added later without changing the core model.
- The control plane never supports an unauthenticated mode.

Authentication uses:

```http
Authorization: Bearer <token>
```

## Runtime Control Handle

Add a runtime-facing handle that centralizes operations which are currently split between `RuntimePluginEngine` and `PluginRuntimeState`.

Initial operations:

- `runtime_snapshot()`
- `plugin_snapshots()`
- `enable_plugin(instance_id)`
- `disable_plugin(instance_id)`
- `start_plugin(instance_id)`
- `stop_plugin(instance_id)`
- `reload_plugin(instance_id)`

The handle is the only object the control plane API receives. This keeps HTTP handlers small and prevents the web layer from editing runtime state directly.

## Lifecycle Semantics

`enabled` controls dispatch eligibility. Lifecycle controls resource ownership.

- `disable_plugin(id)` sets `enabled = false`, calls `stop()` if the plugin is running, and prevents future handler matching.
- `enable_plugin(id)` sets `enabled = true`. If the plugin was initialized before, it calls `start()`; if not, it runs `init()` then `start()`.
- `stop_plugin(id)` calls `stop()` but does not change `enabled`; this is an operational pause.
- `start_plugin(id)` calls `start()` only when the plugin is enabled.
- `reload_plugin(id)` attempts a backend-specific replacement. If a plugin is not reloadable, it returns a structured `not_reloadable` error.

For the first implementation, compiled Rust inventory plugins are `reloadable = false`. Rhai and WASM backends will provide reloadable plugin descriptors later.

## Hot Reload Semantics

Reload must be atomic from the user's point of view:

1. Locate the existing plugin instance and its reload descriptor.
2. Ask the backend to load a candidate plugin.
3. Validate the candidate's manifest against available capabilities and services.
4. Initialize and start the candidate.
5. Swap the candidate into the engine.
6. Stop the old instance.
7. If any candidate step fails, keep the old instance running and record the error on the plugin runtime state.

This reload contract lets the control plane ship before Rhai/WASM while preserving the correct boundary for dynamic backends.

## Control Plane API

All endpoints require bearer-token auth.

```text
GET  /api/runtime
GET  /api/plugins
GET  /api/plugins/:id
POST /api/plugins/:id/enable
POST /api/plugins/:id/disable
POST /api/plugins/:id/start
POST /api/plugins/:id/stop
POST /api/plugins/:id/reload
```

Responses should be structured and stable:

```json
{
  "ok": true,
  "data": {}
}
```

Errors should be machine-readable:

```json
{
  "ok": false,
  "error": {
    "code": "not_reloadable",
    "message": "plugin `echo` is not reloadable"
  }
}
```

## Embedded Management UI

Create a new `webui/` package with Vite, React, TailwindCSS, TypeScript, and Bun.

The UI is an operational dashboard, not a landing page. The first screen should show:

- Runtime status.
- Plugin list with instance id, kind, version, enabled state, lifecycle state, health, and reloadability.
- Required/optional capabilities and services from each manifest.
- Last error and config lifecycle state.
- Actions: enable, disable, start, stop, reload.
- A compact operation result/error area.

Development mode can run Vite separately and proxy API calls to the Rust control plane. Production mode embeds the built static assets behind the control plane server.

## Cargo Features

Add control plane support behind explicit features:

- `control-plane`: enables the HTTP API server.
- `embedded-webui`: includes `control-plane` and serves the built React assets.

The default `ayiou` feature set remains minimal.

## Security Model

- Token is required.
- Default bind address is loopback.
- HTTP handlers never expose stack traces.
- POST is required for state-changing actions.
- Control plane code never receives direct plugin instances.
- Future Rhai/WASM plugins must only access resources through Ayiou host APIs and runtime-granted capabilities.

## Testing Strategy

Rust tests:

- Runtime control handle can disable a plugin and dispatch skips it.
- Disabling a running plugin calls `stop()`.
- Enabling a stopped initialized plugin calls `start()`.
- Reloading a non-reloadable Rust plugin returns `not_reloadable`.
- Control plane rejects missing/invalid bearer tokens.
- API endpoints return stable structured errors.

Frontend tests/build checks:

- `bun run build` for the Vite app.
- API client handles successful and error responses.

Integration checks:

- `cargo test -p ayiou --features control-plane`.
- `cargo test -p ayiou --features embedded-webui` once assets are wired.
- `cargo run -p ayiou --example kitchen_sink` remains unchanged unless the example opts into control plane.

## Open Extension Points

- Rhai backend: loads script files, exposes only registered host functions, and enforces execution/resource limits.
- WASM backend: loads WASM/component artifacts, exposes only Ayiou host imports, and enforces memory/fuel/epoch limits.
- Plugin filesystem watcher: debounced change events trigger `reload_plugin(id)` for reloadable plugins.
- Config file integration: default plugin enabled state and control plane options can be read from host configuration later.
