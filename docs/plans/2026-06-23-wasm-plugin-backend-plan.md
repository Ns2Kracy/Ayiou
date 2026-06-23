# WASM Plugin Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a WASM-only dynamic plugin backend for Ayiou, with hot reload through the existing control-plane lifecycle.

**Architecture:** Keep `RuntimePlugin` as the only runtime-facing plugin model. Add a backend layer that loads WASM component artifacts, adapts them into `RuntimePlugin` instances, and exposes only Ayiou host imports/capabilities. Compiled Rust/inventory plugins remain static and `reloadable = false`; WASM plugins become reloadable through the same `reload_plugin(id)` contract.

**Tech Stack:** Rust, Tokio, Wasmtime component model, WIT contracts, existing `RuntimePluginEngine`, `RuntimeControlHandle`, `RuntimePluginServices`, `PermissionService`, `ConversationStore`, and control-plane HTTP API.

---

## Non-Goals

- Do not support a script-plugin backend in this plan.
- Do not make compiled Rust inventory plugins code-hot-reloadable.
- Do not expose raw `ServiceRegistry` or plugin instances to WASM guests.
- Do not let WASM plugins perform direct host I/O unless Ayiou explicitly exposes it as a host import.
- Do not design a second plugin lifecycle; WASM must fit the existing runtime lifecycle.

## Target Model

```text
Bot
  -> RuntimePluginEngine
      -> compiled Rust/inventory RuntimePlugin instances
      -> WasmPluginBackend
          -> WasmPluginInstance as RuntimePlugin adapter
              -> WASM component artifact
              -> Ayiou host imports
  -> RuntimeControlHandle.reload_plugin(id)
      -> backend-specific atomic replacement
```

Runtime remains unified:

```rust
pub trait RuntimePlugin: Send + Sync + 'static {
    fn kind(&self) -> &str;
    fn manifest(&self) -> RuntimePluginManifest;
    fn declared_handlers(&self) -> Vec<HandlerDecl>;
    async fn init(&mut self, services: RuntimePluginServices) -> anyhow::Result<()>;
    async fn start(&mut self, services: RuntimePluginServices) -> anyhow::Result<()>;
    async fn stop(&mut self) -> anyhow::Result<()>;
    async fn apply_config(&mut self, update: ConfigUpdate) -> anyhow::Result<ApplyConfigOutcome>;
    async fn handle_with_invocation(
        &self,
        ctx: &Context,
        invocation: Option<CommandInvocation>,
    ) -> anyhow::Result<HandleOutcome>;
    fn health(&self) -> PluginHealth;
}
```

WASM implements this contract indirectly through an adapter. Guest code never receives Rust objects.

---

## File Map

### New crate

- Create: `ayiou-wasm/Cargo.toml`
  - Optional backend crate so normal `ayiou` users do not pay Wasmtime compile time unless enabled.
- Create: `ayiou-wasm/src/lib.rs`
  - Public exports for backend, config, errors, generated bindings.
- Create: `ayiou-wasm/src/backend.rs`
  - `WasmPluginBackend`, artifact loading, validation, reload.
- Create: `ayiou-wasm/src/plugin.rs`
  - `WasmRuntimePlugin` adapter implementing `RuntimePlugin`.
- Create: `ayiou-wasm/src/host.rs`
  - Host state and Ayiou host imports available to guest components.
- Create: `ayiou-wasm/src/types.rs`
  - Rust-side DTOs for manifest, handlers, context, outcomes, config.
- Create: `ayiou-wasm/wit/ayiou-plugin.wit`
  - Stable guest/host interface contract.
- Create: `ayiou-wasm/tests/wasm_backend.rs`
  - Backend contract tests with a checked-in fixture component or generated fixture.

### Existing Rust crate

- Modify: `Cargo.toml`
  - Add `ayiou-wasm` as workspace member.
- Modify: `ayiou/Cargo.toml`
  - Add optional feature `wasm-plugins = ["dep:ayiou-wasm"]` if re-exporting from `ayiou` is desired.
- Modify: `ayiou/src/core/plugin.rs`
  - Add reload metadata fields if not already present in snapshots.
  - Keep `RuntimePlugin` unchanged unless adapter needs an explicit backend descriptor.
- Modify: `ayiou/src/core/control.rs`
  - Make `reload_plugin(id)` delegate to backend-specific reload descriptors for reloadable plugins.
- Modify: `ayiou/src/bot.rs`
  - Add builder API for registering WASM plugin directories/artifacts when `wasm-plugins` is enabled.
- Modify: `ayiou/src/lib.rs`
  - Re-export WASM types behind `wasm-plugins` feature if using integrated feature.

### Docs

- Modify: `docs/content/docs/control-plane.mdx`
  - Replace old multi-backend wording with WASM-only wording.
- Modify: `docs/content/docs/getting-started.mdx`
  - Replace old multi-backend wording with WASM-only wording.
- Modify: `docs/plans/2026-06-08-control-plane-hot-reload-design.md`
  - Update historical design to keep WASM as the only dynamic backend extension point.
- Create or update: `docs/content/docs/wasm-plugins.mdx`
  - User-facing WASM plugin contract, security model, build flow, reload behavior.

---

## WIT Contract Draft

Create `ayiou-wasm/wit/ayiou-plugin.wit`:

```wit
package ayiou:plugin;

world plugin {
  import host-log: func(level: string, message: string);
  import send-message: func(target: string, text: string) -> result<string, string>;
  import permission-check: func(permission-kind: string, permission-value: string) -> result<bool, string>;
  import conversation-get: func(key: string) -> result<option<string>, string>;
  import conversation-put: func(key: string, value: string, ttl-ms: option<u64>) -> result<_, string>;
  import conversation-remove: func(key: string) -> result<_, string>;

  export manifest: func() -> string;
  export handlers: func() -> string;
  export init: func(services-json: string) -> result<_, string>;
  export start: func() -> result<_, string>;
  export stop: func() -> result<_, string>;
  export apply-config: func(update-json: string) -> result<string, string>;
  export handle: func(context-json: string, invocation-json: option<string>) -> result<string, string>;
  export health: func() -> string;
}
```

Use JSON payloads for the first version to keep the WIT small and stable while Rust-side DTOs evolve. Move high-traffic fields to typed WIT records only after dispatch behavior stabilizes.

---

## Tasks

### Task 1: Add the `ayiou-wasm` crate skeleton

**Files:**
- Modify: `Cargo.toml`
- Create: `ayiou-wasm/Cargo.toml`
- Create: `ayiou-wasm/src/lib.rs`
- Create: `ayiou-wasm/src/types.rs`
- Create: `ayiou-wasm/wit/ayiou-plugin.wit`

- [ ] **Step 1: Add workspace member**

Edit root `Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = [
    "ayiou",
    "ayiou-macros",
    "ayiou-wasm",
]
```

- [ ] **Step 2: Create backend crate manifest**

Create `ayiou-wasm/Cargo.toml`:

```toml
[package]
name = "ayiou-wasm"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow = "1"
async-trait = "0.1"
ayiou = { path = "../ayiou", default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["sync"] }
wasmtime = { version = "34", features = ["component-model", "async"] }
wasmtime-wasi = "34"
```

If the current latest Wasmtime major differs, pin one version across `wasmtime` and `wasmtime-wasi` after checking official docs.

- [ ] **Step 3: Create library shell**

Create `ayiou-wasm/src/lib.rs`:

```rust
pub mod backend;
pub mod host;
pub mod plugin;
pub mod types;

pub use backend::{WasmPluginBackend, WasmPluginSource};
pub use plugin::WasmRuntimePlugin;
```

- [ ] **Step 4: Create shared DTO types**

Create `ayiou-wasm/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmManifestDto {
    pub kind: String,
    pub version: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmHandlerDto {
    pub commands: Vec<String>,
    pub regex_patterns: Vec<String>,
    pub wildcard: bool,
    pub priority: i32,
    pub block: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmHandleOutcomeDto {
    pub block: bool,
}
```

- [ ] **Step 5: Add WIT contract**

Create `ayiou-wasm/wit/ayiou-plugin.wit` using the contract shown in this plan's WIT section.

- [ ] **Step 6: Verify crate compiles**

Run:

```bash
cargo check -p ayiou-wasm
```

Expected: compile errors only for missing modules from later tasks if `lib.rs` references them. If compiling task-by-task, create empty `backend.rs`, `host.rs`, and `plugin.rs` modules before running.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml ayiou-wasm
git commit -m "feat: add wasm plugin backend crate"
```

### Task 2: Implement WASM artifact source and backend loader

**Files:**
- Create: `ayiou-wasm/src/backend.rs`
- Create: `ayiou-wasm/src/host.rs`
- Modify: `ayiou-wasm/src/lib.rs`

- [ ] **Step 1: Define artifact source**

Create `ayiou-wasm/src/backend.rs`:

```rust
use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use wasmtime::{component::Component, Config, Engine};

#[derive(Debug, Clone)]
pub struct WasmPluginSource {
    pub instance_id: String,
    pub artifact_path: PathBuf,
    pub package_path: Option<PathBuf>,
}

impl WasmPluginSource {
    pub fn new(instance_id: impl Into<String>, artifact_path: impl Into<PathBuf>) -> Self {
        Self {
            instance_id: instance_id.into(),
            artifact_path: artifact_path.into(),
            package_path: None,
        }
    }
}

#[derive(Clone)]
pub struct WasmPluginBackend {
    engine: Arc<Engine>,
}

impl WasmPluginBackend {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config)?;
        Ok(Self { engine: Arc::new(engine) })
    }

    pub fn compile_component(&self, source: &WasmPluginSource) -> Result<Component> {
        Component::from_file(&self.engine, &source.artifact_path)
    }

    pub fn engine(&self) -> Arc<Engine> {
        self.engine.clone()
    }
}
```

- [ ] **Step 2: Define host state**

Create `ayiou-wasm/src/host.rs`:

```rust
use ayiou::core::plugin::RuntimePluginServices;

#[derive(Clone)]
pub struct WasmHostState {
    pub instance_id: String,
    pub services: RuntimePluginServices,
}
```

- [ ] **Step 3: Verify backend compiles**

Run:

```bash
cargo check -p ayiou-wasm
```

Expected: PASS after empty or stub `plugin.rs` exists.

- [ ] **Step 4: Commit**

```bash
git add ayiou-wasm/src/backend.rs ayiou-wasm/src/host.rs ayiou-wasm/src/lib.rs
git commit -m "feat: add wasm plugin backend loader"
```

### Task 3: Implement `WasmRuntimePlugin` adapter

**Files:**
- Create: `ayiou-wasm/src/plugin.rs`
- Modify: `ayiou-wasm/src/types.rs`
- Test: `ayiou-wasm/tests/wasm_backend.rs`

- [ ] **Step 1: Add failing adapter construction test**

Create `ayiou-wasm/tests/wasm_backend.rs`:

```rust
use ayiou_wasm::{WasmPluginBackend, WasmPluginSource};

#[test]
fn wasm_backend_constructs_with_resource_limits_enabled() {
    let backend = WasmPluginBackend::new().expect("backend should construct");
    let _engine = backend.engine();
}
```

- [ ] **Step 2: Run test**

Run:

```bash
cargo test -p ayiou-wasm wasm_backend_constructs_with_resource_limits_enabled --quiet
```

Expected: PASS.

- [ ] **Step 3: Add plugin adapter skeleton**

Create `ayiou-wasm/src/plugin.rs`:

```rust
use anyhow::Result;
use async_trait::async_trait;
use ayiou::core::{
    context::Context,
    plugin::{
        ApplyConfigOutcome, CommandInvocation, ConfigUpdate, HandlerDecl, HandleOutcome,
        PluginHealth, RuntimePlugin, RuntimePluginManifest, RuntimePluginServices,
    },
};

pub struct WasmRuntimePlugin {
    instance_id: String,
    manifest: RuntimePluginManifest,
    handlers: Vec<HandlerDecl>,
}

impl WasmRuntimePlugin {
    pub fn new(
        instance_id: impl Into<String>,
        manifest: RuntimePluginManifest,
        handlers: Vec<HandlerDecl>,
    ) -> Self {
        Self {
            instance_id: instance_id.into(),
            manifest,
            handlers,
        }
    }
}

#[async_trait]
impl RuntimePlugin for WasmRuntimePlugin {
    fn kind(&self) -> &str {
        self.manifest.kind.as_str()
    }

    fn manifest(&self) -> RuntimePluginManifest {
        self.manifest.clone()
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        self.handlers.clone()
    }

    async fn init(&mut self, _services: RuntimePluginServices) -> Result<()> {
        Ok(())
    }

    async fn start(&mut self, _services: RuntimePluginServices) -> Result<()> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn apply_config(&mut self, _update: ConfigUpdate) -> Result<ApplyConfigOutcome> {
        Ok(ApplyConfigOutcome::applied())
    }

    async fn handle_with_invocation(
        &self,
        _ctx: &Context,
        _invocation: Option<CommandInvocation>,
    ) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }

    fn health(&self) -> PluginHealth {
        PluginHealth::healthy()
    }
}
```

- [ ] **Step 4: Run adapter tests**

Run:

```bash
cargo test -p ayiou-wasm --quiet
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add ayiou-wasm/src/plugin.rs ayiou-wasm/tests/wasm_backend.rs
git commit -m "feat: adapt wasm plugins to runtime plugin trait"
```

### Task 4: Bind real component exports to plugin lifecycle

**Files:**
- Modify: `ayiou-wasm/src/backend.rs`
- Modify: `ayiou-wasm/src/plugin.rs`
- Modify: `ayiou-wasm/src/types.rs`
- Test: `ayiou-wasm/tests/wasm_backend.rs`

- [ ] **Step 1: Generate or write bindings**

Use Wasmtime component bindings from `ayiou-wasm/wit/ayiou-plugin.wit`. The implementation should expose a generated `PluginComponent` binding or equivalent typed wrapper.

Expected public loader shape:

```rust
impl WasmPluginBackend {
    pub async fn load_plugin(&self, source: WasmPluginSource) -> anyhow::Result<WasmRuntimePlugin>;
}
```

- [ ] **Step 2: Load manifest and handlers at startup**

`load_plugin` must call guest exports:

```text
manifest() -> JSON RuntimePluginManifest DTO
handlers() -> JSON Vec<WasmHandlerDto>
```

Map DTOs into:

```rust
RuntimePluginManifest::new(kind)
HandlerDecl::message_commands(commands)
```

Regex patterns, wildcard, priority, and block must map to existing `HandlerDecl` fields/builders.

- [ ] **Step 3: Wire lifecycle exports**

`WasmRuntimePlugin` lifecycle methods call guest exports:

```text
init(services-json)
start()
stop()
apply-config(update-json)
handle(context-json, invocation-json)
health()
```

Errors from guest `result<_, string>` become `anyhow!(...)` with `wasm plugin `<instance_id>` ...` context.

- [ ] **Step 4: Test with fixture component**

Add a minimal WASM fixture component that returns:

```json
manifest: { "kind": "fixture-wasm", "version": "0.1.0" }
handlers: [{ "commands": ["ping"], "regex_patterns": [], "wildcard": false, "priority": 0, "block": true }]
handle outcome: { "block": true }
```

Test:

```rust
#[tokio::test]
async fn wasm_backend_loads_manifest_handlers_and_dispatches() {
    // Load fixture through WasmPluginBackend.
    // Register returned plugin in RuntimePluginEngine.
    // Dispatch `/ping` context.
    // Assert handle returns block=true and runtime records no error.
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p ayiou-wasm --quiet
cargo test -p ayiou plugin --quiet
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ayiou-wasm ayiou/tests
git commit -m "feat: execute wasm plugin lifecycle exports"
```

### Task 5: Add host imports for safe Ayiou capabilities

**Files:**
- Modify: `ayiou-wasm/src/host.rs`
- Modify: `ayiou-wasm/src/plugin.rs`
- Test: `ayiou-wasm/tests/wasm_backend.rs`

- [ ] **Step 1: Implement logging import**

Map `host-log(level, message)` to Rust tracing/logging without exposing host internals.

- [ ] **Step 2: Implement outbound send import**

Map `send-message(target, text)` to `RuntimePluginServices::send(...)` only when sender capability exists. Missing sender returns guest-visible error string.

- [ ] **Step 3: Implement permission import**

Map `permission-check(kind, value)` to `PermissionService::check(...)` using:

```rust
Permission::Role(value.to_string())
Permission::Custom(value.to_string())
```

- [ ] **Step 4: Implement conversation imports**

Map conversation imports to `ConversationStore`, scoped by current plugin instance id and current context user/group. Never let guest choose another plugin id.

- [ ] **Step 5: Add host import tests**

Add fixture tests for:

- Guest can log without failure.
- Guest permission check respects host `PermissionService` allow/deny.
- Guest conversation put/get/remove is scoped to its own plugin id.
- Guest send fails clearly when no sender exists.

- [ ] **Step 6: Run tests**

```bash
cargo test -p ayiou-wasm --quiet
cargo test -p ayiou plugin --quiet
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add ayiou-wasm
git commit -m "feat: expose safe host imports to wasm plugins"
```

### Task 6: Integrate WASM reload with runtime control

**Files:**
- Modify: `ayiou/src/core/plugin.rs`
- Modify: `ayiou/src/core/control.rs`
- Modify: `ayiou/src/bot.rs`
- Test: `ayiou/tests/plugin.rs` or `ayiou/tests/control_plane.rs`

- [ ] **Step 1: Add reload descriptor to runtime plugin registration**

Add an internal reload descriptor that records whether a plugin instance is reloadable and which backend owns reload.

Suggested shape:

```rust
pub enum PluginReloadDescriptor {
    NotReloadable,
    Wasm { artifact_path: PathBuf },
}
```

Keep it out of `RuntimePlugin` unless a trait method is clearly simpler.

- [ ] **Step 2: Expose reloadability in snapshots**

Ensure plugin snapshots include:

```rust
pub reloadable: bool
```

Control plane should continue displaying reloadability without knowing backend internals.

- [ ] **Step 3: Implement atomic reload**

`reload_plugin(id)` must:

1. Find old plugin and its reload descriptor.
2. Return `not_reloadable` for Rust/inventory plugins.
3. Ask `WasmPluginBackend` to load a candidate from the same artifact path.
4. Validate manifest, services, capabilities, and handlers.
5. `init` and `start` candidate.
6. Swap candidate into `RuntimePluginEngine`.
7. Rebuild routing table.
8. Stop old plugin.
9. On any failure, keep old plugin running and record candidate error.

- [ ] **Step 4: Add reload tests**

Tests:

```rust
#[tokio::test]
async fn reload_non_reloadable_rust_plugin_returns_not_reloadable() { ... }

#[tokio::test]
async fn reload_wasm_plugin_swaps_candidate_atomically() { ... }

#[tokio::test]
async fn failed_wasm_reload_keeps_old_plugin_running() { ... }
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p ayiou --features control-plane plugin --quiet
cargo test -p ayiou-wasm --quiet
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add ayiou/src/core/plugin.rs ayiou/src/core/control.rs ayiou/src/bot.rs ayiou/tests
git commit -m "feat: reload wasm plugins atomically"
```

### Task 7: Add user-facing WASM plugin docs and remove old multi-backend wording

**Files:**
- Modify: `docs/content/docs/control-plane.mdx`
- Modify: `docs/content/docs/getting-started.mdx`
- Modify: `docs/content/docs/v0-2-kernel-architecture.mdx`
- Modify: `docs/plans/2026-06-08-control-plane-hot-reload-design.md`
- Create: `docs/content/docs/wasm-plugins.mdx`

- [ ] **Step 1: Replace old multi-backend references**

Run repository search for old script-backend names and mixed dynamic-backend wording.

Expected: active docs and plans describe WASM as the only dynamic plugin backend.

- [ ] **Step 2: Document WASM-only architecture**

Create `docs/content/docs/wasm-plugins.mdx` with sections:

- Why WASM for dynamic plugins in a Rust host.
- Compiled Rust plugins vs WASM plugins.
- Artifact loading and reload.
- Host imports and capability limits.
- Security/resource limits.
- Minimal guest contract.

- [ ] **Step 3: Run docs checks**

```bash
cd docs && bun run types:check
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add docs/content/docs docs/plans
git commit -m "docs: plan wasm-only plugin backend"
```

---

## Verification Matrix

Run before merging the full implementation:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --quiet
cargo test --workspace --quiet
cd docs && bun run types:check
```

Expected:

- Rust workspace clippy: PASS.
- Rust workspace tests: PASS.
- Docs typecheck: PASS.
- Search for old script-backend and mixed dynamic-backend wording: no active references.

## Risks and Mitigations

- **Wasmtime API churn:** Pin a Wasmtime major version and isolate usage inside `ayiou-wasm`.
- **Guest ABI instability:** Keep first WIT contract narrow and use JSON DTOs for evolving plugin metadata.
- **Host capability leaks:** Expose only explicit host imports; never pass raw services or registries to guests.
- **Reload race conditions:** Implement reload through `RuntimeControlHandle` and engine-owned atomic swap, not direct backend mutation.
- **Resource exhaustion:** Enable fuel, epoch interruption, memory limits, and clear timeout policy before accepting untrusted plugins.
- **Large compile time:** Keep WASM support in an optional crate/feature.
