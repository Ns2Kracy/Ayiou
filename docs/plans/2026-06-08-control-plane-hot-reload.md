# Control Plane and Hot Reload Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a token-protected Ayiou control plane with plugin lifecycle controls, hot-reload semantics, and an embedded React/Tailwind management UI.

**Architecture:** Keep `RuntimePlugin` as the single plugin model. Add a runtime control layer around `RuntimePluginEngine`, expose it through an optional Axum HTTP control plane, and serve a Vite-built React UI from an `embedded-webui` feature. Rust inventory plugins support lifecycle control but report `not_reloadable` for code reload until Rhai/WASM backends exist.

**Tech Stack:** Rust 2024, tokio, axum, serde, existing `RuntimePluginEngine`, Vite, React, TypeScript, TailwindCSS, Bun.

---

### Task 1: Add Plugin Lifecycle Control Methods to RuntimePluginEngine

**Files:**
- Modify: `ayiou/src/core/plugin_system.rs`

**Step 1: Add failing tests for dispatch and lifecycle control**

Add tests in the existing `#[cfg(test)] mod tests`:

```rust
#[tokio::test]
async fn disabled_plugin_is_stopped_and_skipped_by_dispatch() {
    let host = PluginHost::new(None);
    let services = RuntimePluginServices::new(host);
    let state = PluginRuntimeState::default();
    let stopped = Arc::new(std::sync::Mutex::new(0));
    let handled = Arc::new(std::sync::Mutex::new(0));
    let mut engine = RuntimePluginEngine::new(services, state.clone());
    engine.push_as(
        "switchable",
        Box::new(CountingLifecyclePlugin {
            instance_id: "switchable",
            handler: HandlerDecl::wildcard_message(),
            manifest: RuntimePluginManifest::new("switchable"),
            handled: handled.clone(),
            stopped: stopped.clone(),
        }),
    );

    engine.init_all().await.unwrap();
    engine.start_all().await.unwrap();
    engine.disable_plugin("switchable").await.unwrap();

    assert!(!state.is_enabled("switchable"));
    assert_eq!(*stopped.lock().unwrap(), 1);
    let blocked = engine.handle_all(&TestCtx::default()).await.unwrap();
    assert!(!blocked);
    assert_eq!(*handled.lock().unwrap(), 0);
}

#[tokio::test]
async fn enable_plugin_restarts_initialized_plugin() {
    let host = PluginHost::new(None);
    let services = RuntimePluginServices::new(host);
    let state = PluginRuntimeState::default();
    let started = Arc::new(std::sync::Mutex::new(0));
    let mut engine = RuntimePluginEngine::new(services, state.clone());
    engine.push_as(
        "switchable",
        Box::new(StartCountingPlugin {
            started: started.clone(),
        }),
    );

    engine.init_all().await.unwrap();
    engine.start_all().await.unwrap();
    engine.disable_plugin("switchable").await.unwrap();
    engine.enable_plugin("switchable").await.unwrap();

    assert!(state.is_enabled("switchable"));
    assert_eq!(*started.lock().unwrap(), 2);
    assert_eq!(
        state.snapshot("switchable").lifecycle_state,
        PluginLifecycleState::Running
    );
}

#[tokio::test]
async fn reload_non_reloadable_plugin_reports_structured_error() {
    let host = PluginHost::new(None);
    let services = RuntimePluginServices::new(host);
    let state = PluginRuntimeState::default();
    let mut engine = RuntimePluginEngine::new(services, state);
    engine.push_as("plain", Box::new(PriorityPlugin::wildcard("plain", 0, false)));

    let err = engine.reload_plugin("plain").await.unwrap_err();
    assert!(err.to_string().contains("not reloadable"));
}
```

Create minimal helper plugins near existing test helpers if they do not already exist.

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p ayiou --no-default-features disabled_plugin enable_plugin reload_non_reloadable
```

Expected: FAIL because `disable_plugin`, `enable_plugin`, and `reload_plugin` do not exist.

**Step 3: Implement engine methods**

Add public methods to `RuntimePluginEngine<C>`:

```rust
pub async fn disable_plugin(&mut self, instance_id: &str) -> Result<()>;
pub async fn enable_plugin(&mut self, instance_id: &str) -> Result<()>;
pub async fn stop_plugin(&mut self, instance_id: &str) -> Result<()>;
pub async fn start_plugin(&mut self, instance_id: &str) -> Result<()>;
pub async fn reload_plugin(&mut self, instance_id: &str) -> Result<()>;
```

Implementation rules:

- Look up plugins by `instance_id` and return `plugin instance `<id>` is not registered` if missing.
- `disable_plugin` sets enabled false and calls `stop_plugin` when lifecycle is `Running` or `Starting`.
- `enable_plugin` sets enabled true and calls `start_plugin` when lifecycle is `Stopped` or `Registered` after init.
- `stop_plugin` mirrors the single-plugin behavior from `stop_all`.
- `start_plugin` mirrors the single-plugin behavior from `start_all` and respects enabled state.
- `reload_plugin` returns an `anyhow!` error containing `not reloadable` for now.

**Step 4: Run tests to verify pass**

Run:

```bash
cargo test -p ayiou --no-default-features disabled_plugin enable_plugin reload_non_reloadable
cargo test -p ayiou --no-default-features plugin_snapshots
```

Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/src/core/plugin_system.rs
git commit -m "feat: add plugin lifecycle controls"
```

### Task 2: Add Runtime Control Handle

**Files:**
- Create: `ayiou/src/core/control.rs`
- Modify: `ayiou/src/core.rs`
- Modify: `ayiou/src/bot.rs`

**Step 1: Write failing tests for handle delegation**

Create unit tests in `control.rs` that construct a `RuntimePluginEngine`, wrap it in `Arc<RwLock<_>>`, and verify:

- `plugin_snapshots()` returns snapshots.
- `disable_plugin()` delegates to the engine and updates state.
- `reload_plugin()` returns `not_reloadable`.

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p ayiou --no-default-features runtime_control
```

Expected: FAIL because `core::control` does not exist.

**Step 3: Implement RuntimeControlHandle**

Create:

```rust
#[derive(Clone)]
pub struct RuntimeControlHandle<C> {
    engine: Arc<tokio::sync::RwLock<RuntimePluginEngine<C>>>,
}
```

Add methods:

```rust
pub fn new(engine: Arc<RwLock<RuntimePluginEngine<C>>>) -> Self;
pub async fn plugin_snapshots(&self) -> Vec<RuntimePluginSnapshot>;
pub async fn enable_plugin(&self, id: &str) -> Result<()>;
pub async fn disable_plugin(&self, id: &str) -> Result<()>;
pub async fn start_plugin(&self, id: &str) -> Result<()>;
pub async fn stop_plugin(&self, id: &str) -> Result<()>;
pub async fn reload_plugin(&self, id: &str) -> Result<()>;
```

Export it from `core.rs`.

**Step 4: Wire Bot internals to create the handle**

In `bot.rs`, after the engine becomes `Arc<RwLock<_>>`, create a `RuntimeControlHandle`. Store it where the future control plane server can receive it. If the control plane is not enabled yet, this is a no-op local variable.

**Step 5: Run tests**

Run:

```bash
cargo test -p ayiou --no-default-features runtime_control
cargo test -p ayiou
```

Expected: PASS.

**Step 6: Commit**

```bash
git add ayiou/src/core/control.rs ayiou/src/core.rs ayiou/src/bot.rs
git commit -m "feat: add runtime control handle"
```

### Task 3: Add Control Plane Feature and HTTP API

**Files:**
- Modify: `ayiou/Cargo.toml`
- Create: `ayiou/src/control_plane.rs`
- Modify: `ayiou/src/lib.rs`
- Modify: `ayiou/src/bot.rs`
- Test: `ayiou/tests/control_plane.rs`

**Step 1: Add failing HTTP API tests**

Create `ayiou/tests/control_plane.rs` behind `required-features = ["control-plane"]` in `Cargo.toml`.

Test cases:

- Missing bearer token returns 401.
- Wrong bearer token returns 401.
- `GET /api/plugins` returns plugin snapshots with token.
- `POST /api/plugins/:id/reload` returns structured `not_reloadable`.

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p ayiou --features control-plane --test control_plane
```

Expected: FAIL because the feature/module does not exist.

**Step 3: Add dependencies and feature**

In `ayiou/Cargo.toml` add optional dependencies:

```toml
axum = { version = "0.8", optional = true }
tower = { version = "0.5", optional = true }
http = { version = "1", optional = true }
```

Add feature:

```toml
control-plane = ["dep:axum", "dep:tower", "dep:http", "dep:serde_json"]
```

**Step 4: Implement ControlPlaneOptions and router**

Create `control_plane.rs` with:

```rust
pub struct ControlPlaneOptions {
    pub bind: SocketAddr,
    pub token: Option<String>,
}
```

Builder methods:

- `new()` default bind `127.0.0.1:32187`.
- `bind(...)` parses a socket address.
- `token(...)` stores the token.

Implement router construction:

```rust
pub fn router<C>(handle: RuntimeControlHandle<C>, token: String) -> Router
where
    C: Send + Sync + 'static;
```

Use bearer auth middleware. All responses use `{ ok, data }` or `{ ok, error }`.

**Step 5: Wire Bot builder**

Add to `Bot`:

```rust
#[cfg(feature = "control-plane")]
control_plane_options: Option<ControlPlaneOptions>,
```

Add builder:

```rust
#[cfg(feature = "control-plane")]
pub fn control_plane(mut self, options: ControlPlaneOptions) -> Self;
```

During `run()`, after the control handle exists, spawn the HTTP server when options are present. If token is missing, log a clear error and do not start the bot.

**Step 6: Run tests**

Run:

```bash
cargo test -p ayiou --features control-plane --test control_plane
cargo test -p ayiou --features control-plane
```

Expected: PASS.

**Step 7: Commit**

```bash
git add ayiou/Cargo.toml Cargo.lock ayiou/src/control_plane.rs ayiou/src/lib.rs ayiou/src/bot.rs ayiou/tests/control_plane.rs
git commit -m "feat: add token-protected control plane api"
```

### Task 4: Scaffold Vite React Tailwind Web UI with Bun

**Files:**
- Create: `webui/package.json`
- Create: `webui/index.html`
- Create: `webui/src/main.tsx`
- Create: `webui/src/App.tsx`
- Create: `webui/src/api.ts`
- Create: `webui/src/styles.css`
- Create: `webui/tsconfig.json`
- Create: `webui/vite.config.ts`
- Create: `webui/bun.lock`

**Step 1: Scaffold project**

Run:

```bash
bun create vite webui --template react-ts
```

If the command refuses because the directory exists, create the files manually following the Vite React TypeScript template.

**Step 2: Add TailwindCSS**

Run:

```bash
cd webui && bun add tailwindcss @tailwindcss/vite lucide-react
```

Configure `vite.config.ts` to use the Tailwind Vite plugin.

**Step 3: Build the management UI**

Implement a direct dashboard:

- Runtime heading bar.
- Token input persisted in local storage for development.
- Plugin table/cards with enabled, lifecycle, health, reloadable, last error.
- Action buttons using lucide icons for enable, disable, start, stop, reload.
- API error/result strip.

Keep layout dense and operational. Do not create a landing page.

**Step 4: Add API client**

`api.ts` should implement:

```ts
export async function listPlugins(token: string): Promise<PluginSnapshot[]>;
export async function pluginAction(token: string, id: string, action: PluginAction): Promise<void>;
```

Handle `{ ok: false, error }` consistently.

**Step 5: Verify build**

Run:

```bash
cd webui && bun run build
```

Expected: PASS and creates `webui/dist`.

**Step 6: Commit**

```bash
git add webui
git commit -m "feat: add control plane web ui"
```

### Task 5: Embed Web UI Assets Behind Feature

**Files:**
- Modify: `ayiou/Cargo.toml`
- Modify: `ayiou/src/control_plane.rs`
- Create or modify: `ayiou/build.rs`

**Step 1: Add failing embedded route test**

In `ayiou/tests/control_plane.rs`, add a test under `embedded-webui` that calls `GET /` and expects an HTML response.

**Step 2: Run test to verify failure**

Run:

```bash
cargo test -p ayiou --features embedded-webui --test control_plane
```

Expected: FAIL because embedded web UI feature does not exist.

**Step 3: Add feature and asset embedding**

In `Cargo.toml` add:

```toml
include_dir = { version = "0.7", optional = true }
mime_guess = { version = "2", optional = true }

embedded-webui = ["control-plane", "dep:include_dir", "dep:mime_guess"]
```

Serve `webui/dist` assets from the control plane router under `#[cfg(feature = "embedded-webui")]`.

**Step 4: Verify Rust and UI builds**

Run:

```bash
cd webui && bun run build
cargo test -p ayiou --features embedded-webui --test control_plane
cargo test -p ayiou --features embedded-webui
```

Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/Cargo.toml Cargo.lock ayiou/src/control_plane.rs ayiou/build.rs webui/dist
git commit -m "feat: embed control plane web ui"
```

### Task 6: Documentation and Final Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/content/docs/getting-started.mdx`
- Create: `docs/content/docs/control-plane.mdx`
- Modify: `docs/content/docs/meta.json`

**Step 1: Document control plane usage**

Add examples for:

```rust
use ayiou::{Bot, ControlPlaneOptions};

Bot::new(adapter)
    .control_plane(ControlPlaneOptions::new().token("change-me"))
    .run()
    .await;
```

State clearly:

- Token is required.
- Bind host is optional and defaults to loopback.
- Rust plugins can be controlled but are not code-reloadable.
- Rhai/WASM reloadable backend is future work using the same control API.

**Step 2: Run full verification**

Run:

```bash
cargo fmt
cargo test
cargo test -p ayiou --features control-plane
cargo test -p ayiou --features embedded-webui
cargo run -p ayiou --example kitchen_sink
cd webui && bun run build
```

Expected: PASS.

**Step 3: Commit docs**

```bash
git add README.md docs/content/docs/getting-started.mdx docs/content/docs/control-plane.mdx docs/content/docs/meta.json
git commit -m "docs: document control plane"
```

**Step 4: Push branch**

```bash
git push origin main
```

Expected: `main -> main`.
