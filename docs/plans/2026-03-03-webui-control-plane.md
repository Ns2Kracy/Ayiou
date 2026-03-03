# Ayiou WebUI Control Plane Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a centralized WebUI control plane that can manage multiple bot instances at runtime (start/stop bot, enable/disable plugins, edit plugin config, view logs/metrics) without bot restarts.

**Architecture:** Introduce three new layers: shared admin protocol, per-bot agent, and centralized control plane. Extend `ayiou` runtime with lifecycle control + plugin runtime gates + hot config update. Deliver WebUI in the control plane and keep V2 extensibility for Wasm dynamic plugins (`command + regex + cron`).

**Tech Stack:** Rust 2024, Tokio, Axum, Serde, UUID, DashMap, SQLx (sqlite/postgres), redis, `toml`, Askama templates (WebUI), Wasmtime (V2).

---

Execution rules for implementer:
- Follow `@superpowers/test-driven-development` for every behavior change.
- Follow `@superpowers/verification-before-completion` before any completion claim.
- Use frequent commits (one commit per task).

### Task 1: Add Shared Admin Protocol Crate

**Files:**
- Create: `ayiou-admin-proto/Cargo.toml`
- Create: `ayiou-admin-proto/src/lib.rs`
- Modify: `Cargo.toml`
- Test: `ayiou-admin-proto/src/lib.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn command_envelope_json_roundtrip() {
    let msg = CommandEnvelope::new(
        "cmd-1",
        "bot-a",
        AdminCommand::EnablePlugin { plugin_name: "echo".into() },
    );

    let json = serde_json::to_string(&msg).unwrap();
    let back: CommandEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(back.command_id, "cmd-1");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-admin-proto command_envelope_json_roundtrip -v`
Expected: FAIL because crate/types do not exist yet.

**Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdminCommand {
    StartBot,
    StopBot,
    EnablePlugin { plugin_name: String },
    DisablePlugin { plugin_name: String },
    UpdatePluginConfig {
        plugin_name: String,
        backend: ConfigBackend,
        content: String,
        expected_version: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub command_id: String,
    pub bot_id: String,
    pub command: AdminCommand,
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-admin-proto -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add Cargo.toml ayiou-admin-proto
git commit -m "feat: add shared admin protocol crate"
```

### Task 2: Add Runtime Lifecycle Controller to `ayiou`

**Files:**
- Create: `ayiou/src/core/runtime.rs`
- Modify: `ayiou/src/core.rs`
- Modify: `ayiou/src/lib.rs`
- Test: `ayiou/src/core/runtime.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn runtime_state_transitions_are_idempotent() {
    let rt = RuntimeController::default();
    assert!(rt.start().await.is_ok());
    assert!(rt.start().await.is_ok()); // idempotent
    assert!(rt.stop().await.is_ok());
    assert!(rt.stop().await.is_ok()); // idempotent
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou runtime_state_transitions_are_idempotent -v`
Expected: FAIL (`RuntimeController` missing).

**Step 3: Write minimal implementation**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeState { Running, Stopped }

#[derive(Default)]
pub struct RuntimeController {
    state: Arc<tokio::sync::RwLock<RuntimeState>>,
}

impl RuntimeController {
    pub async fn start(&self) -> anyhow::Result<()> { /* set Running */ }
    pub async fn stop(&self) -> anyhow::Result<()> { /* set Stopped */ }
    pub async fn state(&self) -> RuntimeState { *self.state.read().await }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou runtime_state_transitions_are_idempotent -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/src/core/runtime.rs ayiou/src/core.rs ayiou/src/lib.rs
git commit -m "feat: add runtime lifecycle controller for bot start-stop"
```

### Task 3: Add Plugin Enable/Disable Runtime Gate

**Files:**
- Create: `ayiou/src/core/plugin_runtime.rs`
- Modify: `ayiou/src/core.rs`
- Modify: `ayiou/src/core/plugin.rs`
- Test: `ayiou/src/core/plugin.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn disabled_plugin_is_not_dispatched() {
    let state = PluginRuntimeState::default();
    state.set_enabled("echo", false);

    let plugin = Arc::new(TestPlugin::named("echo"));
    let dispatcher = Dispatcher::with_runtime_state([plugin], DispatchOptions::default(), state);

    dispatcher.dispatch(&TestCtx::from_text("echo hi")).await.unwrap();
    assert_eq!(TestPlugin::handled_times("echo"), 0);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou disabled_plugin_is_not_dispatched -v`
Expected: FAIL (`PluginRuntimeState` / constructor missing).

**Step 3: Write minimal implementation**

```rust
#[derive(Default, Clone)]
pub struct PluginRuntimeState {
    enabled: Arc<dashmap::DashMap<String, bool>>,
}

impl PluginRuntimeState {
    pub fn set_enabled(&self, plugin: &str, on: bool) { self.enabled.insert(plugin.into(), on); }
    pub fn is_enabled(&self, plugin: &str) -> bool { self.enabled.get(plugin).map(|v| *v).unwrap_or(true) }
}
```

In dispatcher loop, skip plugin when `!runtime_state.is_enabled(&plugin.meta().name)`.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou plugin::tests:: -v`
Expected: PASS, including existing dispatch tests.

**Step 5: Commit**

```bash
git add ayiou/src/core/plugin_runtime.rs ayiou/src/core.rs ayiou/src/core/plugin.rs
git commit -m "feat: support runtime plugin enable-disable gate"
```

### Task 4: Add ConfigStore Trait and TOML Backend (Default)

**Files:**
- Create: `ayiou/src/core/config_store.rs`
- Modify: `ayiou/src/core.rs`
- Test: `ayiou/src/core/config_store.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn toml_store_persists_and_versions_config() {
    let dir = tempfile::tempdir().unwrap();
    let store = TomlConfigStore::new(dir.path());

    let v1 = store.put("bot-a", "echo", "key='v1'", None).await.unwrap();
    let err = store.put("bot-a", "echo", "key='v2'", Some(v1 + 1)).await.unwrap_err();
    assert!(err.to_string().contains("version conflict"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou toml_store_persists_and_versions_config -v`
Expected: FAIL (`TomlConfigStore` missing).

**Step 3: Write minimal implementation**

```rust
#[async_trait::async_trait]
pub trait ConfigStore: Send + Sync {
    async fn get(&self, bot_id: &str, plugin: &str) -> anyhow::Result<Option<ConfigRecord>>;
    async fn put(
        &self,
        bot_id: &str,
        plugin: &str,
        content: &str,
        expected_version: Option<u64>,
    ) -> anyhow::Result<u64>;
}
```

Implement `TomlConfigStore` with one file per `(bot, plugin)` and optimistic version check.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou config_store::tests:: -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/src/core/config_store.rs ayiou/src/core.rs
git commit -m "feat: add config store trait with default toml backend"
```

### Task 5: Create `ayiou-agent` Crate and Command Executor

**Files:**
- Create: `ayiou-agent/Cargo.toml`
- Create: `ayiou-agent/src/lib.rs`
- Create: `ayiou-agent/src/executor.rs`
- Modify: `Cargo.toml`
- Test: `ayiou-agent/src/executor.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn duplicate_command_id_is_executed_once() {
    let runtime = FakeRuntime::default();
    let exec = CommandExecutor::new(runtime.clone());

    let cmd = CommandEnvelope::new("c1", "bot-a", AdminCommand::StartBot);
    exec.execute(cmd.clone()).await.unwrap();
    exec.execute(cmd).await.unwrap();

    assert_eq!(runtime.start_calls(), 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-agent duplicate_command_id_is_executed_once -v`
Expected: FAIL (crate missing).

**Step 3: Write minimal implementation**

```rust
pub struct CommandExecutor<R> {
    runtime: R,
    seen: DashSet<String>,
}

impl<R: RuntimeOps> CommandExecutor<R> {
    pub async fn execute(&self, env: CommandEnvelope) -> anyhow::Result<CommandAck> {
        if !self.seen.insert(env.command_id.clone()) {
            return Ok(CommandAck::already_applied(env.command_id));
        }
        // dispatch to runtime
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-agent -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add Cargo.toml ayiou-agent
git commit -m "feat: add bot agent command executor with idempotency"
```

### Task 6: Build Control Plane Backend + RBAC/Auth Foundation

**Files:**
- Create: `ayiou-control-plane/Cargo.toml`
- Create: `ayiou-control-plane/src/main.rs`
- Create: `ayiou-control-plane/src/app.rs`
- Create: `ayiou-control-plane/src/auth.rs`
- Create: `ayiou-control-plane/src/rbac.rs`
- Modify: `Cargo.toml`
- Test: `ayiou-control-plane/tests/auth_rbac.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn user_without_plugin_disable_permission_gets_403() {
    let app = test_app_with_seed_user("viewer", &["logs:read"]);
    let res = app
        .post("/api/v1/bots/bot-a/plugins/echo/disable")
        .auth("viewer-token")
        .send()
        .await;

    assert_eq!(res.status(), 403);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-control-plane auth_rbac -v`
Expected: FAIL (crate/route missing).

**Step 3: Write minimal implementation**

```rust
pub fn require(permission: &'static str) -> impl axum::extract::FromRequestParts<AppState> {
    // decode token -> load roles -> ensure permission exists
}

Router::new().route(
    "/api/v1/bots/:id/plugins/:name/disable",
    post(disable_plugin).route_layer(require("plugin:disable")),
);
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-control-plane auth_rbac -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add Cargo.toml ayiou-control-plane
git commit -m "feat: bootstrap control plane with auth and rbac"
```

### Task 7: Implement Bot Registry + Agent Session + Command Dispatch APIs

**Files:**
- Create: `ayiou-control-plane/src/bot_registry.rs`
- Create: `ayiou-control-plane/src/agent_session.rs`
- Modify: `ayiou-control-plane/src/app.rs`
- Modify: `ayiou-control-plane/src/main.rs`
- Test: `ayiou-control-plane/tests/bot_commands.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn start_bot_endpoint_pushes_command_to_connected_agent() {
    let (app, fake_agent) = test_app_with_connected_agent("bot-a");
    let res = app.post("/api/v1/bots/bot-a/start").auth("admin-token").send().await;

    assert_eq!(res.status(), 202);
    assert_eq!(fake_agent.last_command().await.command, AdminCommand::StartBot);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-control-plane bot_commands -v`
Expected: FAIL (registry/dispatch missing).

**Step 3: Write minimal implementation**

```rust
pub struct BotRegistry {
    sessions: DashMap<String, AgentSessionHandle>,
}

impl BotRegistry {
    pub async fn send_command(&self, bot_id: &str, cmd: AdminCommand) -> anyhow::Result<()> {
        let env = CommandEnvelope::new(uuid::Uuid::new_v4().to_string(), bot_id.to_string(), cmd);
        self.sessions.get(bot_id).ok_or_else(|| anyhow!("agent offline"))?.send(env).await
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-control-plane bot_commands -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou-control-plane/src/bot_registry.rs ayiou-control-plane/src/agent_session.rs ayiou-control-plane/src/app.rs ayiou-control-plane/src/main.rs
git commit -m "feat: add bot registry and control command dispatch"
```

### Task 8: Implement Plugin State + Config APIs with Hot Update Flow

**Files:**
- Create: `ayiou-control-plane/src/plugin_service.rs`
- Modify: `ayiou-control-plane/src/app.rs`
- Modify: `ayiou-control-plane/src/bot_registry.rs`
- Test: `ayiou-control-plane/tests/plugin_config_hot_update.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn update_plugin_config_persists_and_dispatches_command() {
    let (app, fake_agent, store) = test_app_with_store_and_agent();

    let res = app
      .put("/api/v1/bots/bot-a/plugins/echo/config")
      .json(serde_json::json!({"backend":"toml","content":"threshold=3","expected_version":null}))
      .auth("admin-token")
      .send()
      .await;

    assert_eq!(res.status(), 202);
    assert!(store.get("bot-a", "echo").await.unwrap().is_some());
    assert!(matches!(fake_agent.last_command().await.command, AdminCommand::UpdatePluginConfig { .. }));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-control-plane plugin_config_hot_update -v`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
async fn update_plugin_config(...) -> Result<StatusCode, ApiError> {
    let version = state.config_store
        .put(&bot_id, &plugin_name, &req.content, req.expected_version)
        .await?;

    state.bot_registry
        .send_command(&bot_id, AdminCommand::UpdatePluginConfig {
            plugin_name,
            backend: req.backend,
            content: req.content,
            expected_version: Some(version),
        })
        .await?;

    Ok(StatusCode::ACCEPTED)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-control-plane plugin_config_hot_update -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou-control-plane/src/plugin_service.rs ayiou-control-plane/src/app.rs ayiou-control-plane/src/bot_registry.rs
git commit -m "feat: add plugin enable-disable and config hot update apis"
```

### Task 9: Add Logs/Metrics Ingest + Query APIs

**Files:**
- Create: `ayiou-control-plane/src/observability.rs`
- Modify: `ayiou-control-plane/src/app.rs`
- Test: `ayiou-control-plane/tests/observability_api.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn metrics_query_returns_latest_plugin_counters() {
    let app = test_app();

    app.post("/internal/v1/ingest/metrics")
        .json(serde_json::json!({"bot_id":"bot-a","name":"plugin_errors_total","value":2,"labels":{"plugin":"echo"}}))
        .send()
        .await;

    let res = app.get("/api/v1/bots/bot-a/metrics").auth("viewer-token").send().await;
    assert_eq!(res.status(), 200);
    assert!(res.text().await.contains("plugin_errors_total"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-control-plane observability_api -v`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
#[derive(Default)]
pub struct MetricsStore { counters: DashMap<String, u64> }

pub async fn ingest_metric(State(s): State<AppState>, Json(e): Json<MetricEvent>) -> StatusCode {
    s.metrics.upsert(e);
    StatusCode::ACCEPTED
}

pub async fn query_metrics(...) -> Json<Vec<MetricPoint>> { /* filter by bot */ }
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-control-plane observability_api -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou-control-plane/src/observability.rs ayiou-control-plane/src/app.rs
git commit -m "feat: add log and metrics ingest-query apis"
```

### Task 10: Deliver WebUI Pages (Bot, Plugin, Config, Logs, Metrics)

**Files:**
- Create: `ayiou-control-plane/src/webui.rs`
- Create: `ayiou-control-plane/templates/layout.html`
- Create: `ayiou-control-plane/templates/bots.html`
- Create: `ayiou-control-plane/templates/bot_detail.html`
- Modify: `ayiou-control-plane/src/app.rs`
- Test: `ayiou-control-plane/tests/webui_pages.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn bot_detail_page_contains_runtime_actions() {
    let app = test_app_with_bot("bot-a");
    let res = app.get("/ui/bots/bot-a").auth("admin-token").send().await;

    assert_eq!(res.status(), 200);
    let body = res.text().await;
    assert!(body.contains("Start Bot"));
    assert!(body.contains("Stop Bot"));
    assert!(body.contains("Enable Plugin"));
    assert!(body.contains("Save Config"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-control-plane webui_pages -v`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/ui/bots", get(bots_page))
        .route("/ui/bots/:id", get(bot_detail_page))
}
```

Create Askama templates with forms/buttons wired to existing APIs.

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-control-plane webui_pages -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou-control-plane/src/webui.rs ayiou-control-plane/templates ayiou-control-plane/src/app.rs
git commit -m "feat: add control plane webui for bot and plugin management"
```

### Task 11: Add Config Backends (`sqlite` / `redis` / `postgres`) Behind Features

**Files:**
- Create: `ayiou-control-plane/src/config_store/mod.rs`
- Create: `ayiou-control-plane/src/config_store/sqlite.rs`
- Create: `ayiou-control-plane/src/config_store/redis.rs`
- Create: `ayiou-control-plane/src/config_store/postgres.rs`
- Modify: `ayiou-control-plane/Cargo.toml`
- Test: `ayiou-control-plane/tests/config_store_contract.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn sqlite_backend_passes_config_store_contract() {
    let store = sqlite_test_store().await;
    run_config_store_contract(store).await;
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou-control-plane sqlite_backend_passes_config_store_contract -v`
Expected: FAIL.

**Step 3: Write minimal implementation**

```rust
pub enum StoreBackend {
    Toml(TomlConfigStore),
    Sqlite(SqliteConfigStore),
    Redis(RedisConfigStore),
    Postgres(PostgresConfigStore),
}

#[async_trait::async_trait]
impl ConfigStore for StoreBackend { /* delegate */ }
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou-control-plane config_store_contract -v`
Expected: PASS for enabled backends.

**Step 5: Commit**

```bash
git add ayiou-control-plane/src/config_store ayiou-control-plane/Cargo.toml
git commit -m "feat: add pluggable config store backends"
```

### Task 12 (V2): Add Wasm Plugin Runtime (`command + regex + cron`)

**Files:**
- Create: `ayiou/src/core/wasm/mod.rs`
- Create: `ayiou/src/core/wasm/runtime.rs`
- Create: `ayiou/src/core/wasm/host_api.rs`
- Create: `ayiou-wasm-sdk/Cargo.toml`
- Create: `ayiou-wasm-sdk/src/lib.rs`
- Modify: `ayiou/src/core.rs`
- Modify: `ayiou/src/core/plugin.rs`
- Test: `ayiou/src/core/wasm/runtime.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn wasm_plugin_handles_command_regex_and_cron() {
    let rt = WasmRuntime::new(test_host());
    rt.load_module("fixtures/echo_plugin.wasm").await.unwrap();

    assert!(rt.dispatch_command("echo", "hi").await.unwrap());
    assert!(rt.dispatch_regex("https://example.com").await.unwrap());
    assert!(rt.trigger_cron("*/1 * * * * *").await.unwrap());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p ayiou wasm_plugin_handles_command_regex_and_cron -v`
Expected: FAIL (`WasmRuntime` missing).

**Step 3: Write minimal implementation**

```rust
pub struct WasmRuntime { engine: wasmtime::Engine, store: wasmtime::Store<HostState> }

impl WasmRuntime {
    pub async fn load_module(&self, path: &str) -> anyhow::Result<()> { /* instantiate module */ }
    pub async fn dispatch_command(&self, cmd: &str, args: &str) -> anyhow::Result<bool> { /* host call */ }
    pub async fn dispatch_regex(&self, text: &str) -> anyhow::Result<bool> { /* host call */ }
    pub async fn trigger_cron(&self, expr: &str) -> anyhow::Result<bool> { /* host call */ }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p ayiou wasm::tests:: -v`
Expected: PASS.

**Step 5: Commit**

```bash
git add ayiou/src/core/wasm ayiou/src/core.rs ayiou/src/core/plugin.rs ayiou-wasm-sdk
git commit -m "feat: add wasm plugin runtime with command regex cron support"
```

### Final Verification Checklist

1. Run unit and integration tests:
- `cargo test -p ayiou-admin-proto -v`
- `cargo test -p ayiou -v`
- `cargo test -p ayiou-agent -v`
- `cargo test -p ayiou-control-plane -v`

2. Run full workspace verification:
- `cargo test --workspace -v`

3. Manual smoke flow:
- Start control plane
- Register/connect one agent
- From WebUI perform: start bot -> disable plugin -> edit config -> query logs/metrics
- Confirm no bot process restart occurred

4. Capture evidence in PR description:
- API screenshots / curl output
- WebUI screenshots
- test output summary
