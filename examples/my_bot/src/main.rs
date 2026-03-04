use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use axum::serve;
use ayiou::{
    NoopWasmHost, RuntimeController, RuntimeState, WasmRuntime,
    adapter::onebot::v11::{adapter::OneBotV11Adapter, ctx::Ctx},
    core::{DispatchOptions, Dispatcher, PluginManager, PluginRuntimeState, adapter::Adapter},
};
use ayiou_admin_proto::{CommandEnvelope, ConfigBackend};
use ayiou_agent::executor::{CommandExecutor, RuntimeOps};
use ayiou_control_plane::{
    agent_session::{AgentSession, AgentSessionHandle},
    app::{AppState, build_router},
    config_store::{
        ConfigStore as ControlPlaneConfigStore, InMemoryConfigStore, StoreBackend,
        postgres::PostgresConfigStore, redis::RedisConfigStore, sqlite::SqliteConfigStore,
    },
    observability::{MetricEvent, MetricsStore},
};
use ayiou_plugin_qweather::QWeatherPlugin;
use log::{error, info, warn};
use tokio::sync::{Notify, RwLock};

mod plugin;

use plugin::{AddPlugin, EchoPlugin, GuessPlugin, ToolboxPlugin, UrlDetectorPlugin, WhoamiPlugin};

#[derive(Clone, Default)]
struct MyBotRuntimeOps {
    runtime: RuntimeController,
    plugins: PluginRuntimeState,
    wasm: WasmRuntime,
    configs: Arc<RwLock<HashMap<String, RuntimeConfig>>>,
    wasm_alias: Arc<RwLock<HashMap<String, String>>>,
}

#[derive(Clone)]
struct RuntimeConfig {
    version: u64,
}

impl MyBotRuntimeOps {
    pub async fn is_running(&self) -> bool {
        self.runtime.state().await == RuntimeState::Running
    }
}

#[async_trait]
impl RuntimeOps for MyBotRuntimeOps {
    async fn start_bot(&self, _bot_id: &str) -> Result<()> {
        self.runtime.start().await
    }

    async fn stop_bot(&self, _bot_id: &str) -> Result<()> {
        self.runtime.stop().await
    }

    async fn set_plugin_enabled(
        &self,
        _bot_id: &str,
        plugin_name: &str,
        enabled: bool,
    ) -> Result<()> {
        self.plugins.set_enabled(plugin_name, enabled);
        Ok(())
    }

    async fn update_plugin_config(
        &self,
        _bot_id: &str,
        plugin_name: &str,
        _backend: ConfigBackend,
        _content: &str,
        expected_version: Option<u64>,
    ) -> Result<()> {
        let mut configs = self.configs.write().await;
        let current = configs.get(plugin_name).cloned();
        let actual = current.as_ref().map_or(0, |cfg| cfg.version);

        let next = match expected_version {
            Some(version) if version < actual => {
                bail!("version conflict: expected {}, actual {}", version, actual)
            }
            Some(version) => version,
            None => actual
                .checked_add(1)
                .context("config version overflow in my_bot runtime")?,
        };

        configs.insert(plugin_name.to_string(), RuntimeConfig { version: next });
        Ok(())
    }

    async fn load_wasm_plugin(
        &self,
        _bot_id: &str,
        plugin_name: &str,
        module_path: &str,
    ) -> Result<()> {
        let before: HashSet<String> = self.wasm.loaded_modules().await.into_iter().collect();
        self.wasm
            .load_module(module_path)
            .await
            .with_context(|| format!("load wasm module from {}", module_path))?;
        let after: HashSet<String> = self.wasm.loaded_modules().await.into_iter().collect();

        let resolved = if after.contains(plugin_name) {
            plugin_name.to_string()
        } else {
            let diff: Vec<String> = after.difference(&before).cloned().collect();
            if diff.len() != 1 {
                bail!(
                    "loaded wasm module could not be mapped to plugin '{}'",
                    plugin_name
                );
            }
            diff[0].clone()
        };

        self.wasm_alias
            .write()
            .await
            .insert(plugin_name.to_string(), resolved);
        Ok(())
    }

    async fn unload_wasm_plugin(&self, _bot_id: &str, plugin_name: &str) -> Result<()> {
        let module_name = self
            .wasm_alias
            .read()
            .await
            .get(plugin_name)
            .cloned()
            .unwrap_or_else(|| plugin_name.to_string());

        let removed = self
            .wasm
            .unload_module(&module_name)
            .await
            .with_context(|| format!("unload wasm module {}", module_name))?;
        if !removed {
            bail!("wasm module '{}' is not loaded", module_name);
        }

        self.wasm_alias.write().await.remove(plugin_name);
        Ok(())
    }
}

#[derive(Clone)]
struct LoopbackAgentSession {
    executor: Arc<CommandExecutor<MyBotRuntimeOps>>,
}

impl LoopbackAgentSession {
    fn new(executor: Arc<CommandExecutor<MyBotRuntimeOps>>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl AgentSession for LoopbackAgentSession {
    async fn send(&self, command: CommandEnvelope) -> Result<()> {
        self.executor.execute(command).await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    unsafe {
        std::env::set_var("RUST_LOG", "DEBUG");
    }

    pretty_env_logger::try_init().ok();

    let onebot_ws_url =
        std::env::var("ONEBOT_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:3001".to_string());
    let bot_id = std::env::var("MY_BOT_ID").unwrap_or_else(|_| "my-bot".to_string());
    let control_plane_addr = std::env::var("CONTROL_PLANE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:7788".to_string())
        .parse::<SocketAddr>()
        .context("parse CONTROL_PLANE_ADDR as socket address")?;
    let control_plane_token =
        std::env::var("CONTROL_PLANE_TOKEN").unwrap_or_else(|_| "admin-token".to_string());

    let runtime_ops = MyBotRuntimeOps {
        runtime: RuntimeController::default(),
        plugins: PluginRuntimeState::default(),
        wasm: WasmRuntime::new(NoopWasmHost),
        configs: Arc::new(RwLock::new(HashMap::new())),
        wasm_alias: Arc::new(RwLock::new(HashMap::new())),
    };

    let executor = Arc::new(CommandExecutor::new(runtime_ops.clone()));
    let (control_plane_state, metrics_store) = build_control_plane_state(&control_plane_token)?;
    control_plane_state.bot_registry().register(
        bot_id.clone(),
        AgentSessionHandle::new(LoopbackAgentSession::new(executor)),
    );

    info!(
        "Control Plane WebUI: http://{}/ui/login",
        control_plane_addr
    );
    info!("Control Plane token: {}", control_plane_token);
    info!("Managed bot id: {}", bot_id);
    info!("OneBot endpoint: {}", onebot_ws_url);

    let shutdown = Arc::new(Notify::new());
    let server_shutdown = shutdown.clone();
    let bot_shutdown = shutdown.clone();
    let runtime_for_bot = runtime_ops.clone();
    let bot_id_for_metrics = bot_id.clone();
    let onebot_for_bot = onebot_ws_url.clone();
    let metrics_for_bot = metrics_store.clone();

    let control_plane_task = tokio::spawn(async move {
        run_control_plane(control_plane_addr, control_plane_state, server_shutdown).await
    });

    let bot_task = tokio::spawn(async move {
        run_managed_onebot_bot(
            runtime_for_bot,
            onebot_for_bot,
            bot_id_for_metrics,
            metrics_for_bot,
            bot_shutdown,
        )
        .await
    });

    let mut control_plane_task = control_plane_task;
    let mut bot_task = bot_task;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down my_bot services...");
            shutdown.notify_waiters();
        }
        result = &mut control_plane_task => {
            shutdown.notify_waiters();
            let control_plane_result = result.context("join control-plane task")?;
            let bot_result = bot_task.await.context("join bot task")?;
            control_plane_result?;
            bot_result?;
            return Ok(());
        }
        result = &mut bot_task => {
            shutdown.notify_waiters();
            let bot_result = result.context("join bot task")?;
            let control_plane_result = control_plane_task.await.context("join control-plane task")?;
            control_plane_result?;
            bot_result?;
            return Ok(());
        }
    }

    let control_plane_result = control_plane_task
        .await
        .context("join control-plane task")?;
    let bot_result = bot_task.await.context("join bot task")?;
    control_plane_result?;
    bot_result?;
    Ok(())
}

fn build_control_plane_state(token: &str) -> Result<(AppState, MetricsStore)> {
    let permissions = [
        "bot:start",
        "bot:stop",
        "plugin:enable",
        "plugin:disable",
        "plugin:load",
        "plugin:unload",
        "config:write",
        "metrics:read",
    ];
    let state = AppState::single_user("admin", token, &permissions)
        .with_config_store(build_control_plane_store()?);
    let metrics_store = state.metrics_store();
    Ok((state, metrics_store))
}

fn build_control_plane_store() -> Result<Arc<dyn ControlPlaneConfigStore>> {
    let backend = std::env::var("CONTROL_PLANE_CONFIG_BACKEND")
        .unwrap_or_else(|_| "sqlite".to_string())
        .to_lowercase();

    match backend.as_str() {
        "memory" | "in-memory" => Ok(Arc::new(StoreBackend::InMemory(
            InMemoryConfigStore::default(),
        ))),
        "sqlite" => {
            let relative = std::env::var("CONTROL_PLANE_SQLITE_PATH")
                .unwrap_or_else(|_| ".ayiou/my_bot/control_plane.db".to_string());
            let sqlite_url = sqlite_url_from_path(relative)?;
            Ok(Arc::new(StoreBackend::Sqlite(SqliteConfigStore::new(
                sqlite_url,
            ))))
        }
        "redis" => {
            let redis_url = std::env::var("CONTROL_PLANE_REDIS_URL")
                .context("set CONTROL_PLANE_REDIS_URL when backend=redis")?;
            let store = RedisConfigStore::new(redis_url)?;
            Ok(Arc::new(StoreBackend::Redis(store)))
        }
        "postgres" => {
            let dsn = std::env::var("CONTROL_PLANE_POSTGRES_DSN")
                .context("set CONTROL_PLANE_POSTGRES_DSN when backend=postgres")?;
            Ok(Arc::new(StoreBackend::Postgres(PostgresConfigStore::new(
                dsn,
            ))))
        }
        other => bail!(
            "unsupported CONTROL_PLANE_CONFIG_BACKEND '{}', expected sqlite|redis|postgres|memory",
            other
        ),
    }
}

fn sqlite_url_from_path(path: String) -> Result<String> {
    if path.starts_with("sqlite:") {
        return Ok(path);
    }

    let db_path = PathBuf::from(path);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "create sqlite parent directory {}",
                parent.to_string_lossy()
            )
        })?;
    }
    Ok(format!("sqlite://{}", db_path.to_string_lossy()))
}

async fn run_control_plane(addr: SocketAddr, state: AppState, shutdown: Arc<Notify>) -> Result<()> {
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind control-plane listener {}", addr))?;
    serve(listener, app)
        .with_graceful_shutdown(async move { shutdown.notified().await })
        .await
        .context("run control-plane server")
}

async fn run_managed_onebot_bot(
    runtime_ops: MyBotRuntimeOps,
    onebot_ws_url: String,
    bot_id: String,
    metrics_store: MetricsStore,
    shutdown: Arc<Notify>,
) -> Result<()> {
    runtime_ops.start_bot(&bot_id).await?;

    let mut plugins = PluginManager::<Ctx>::new();
    plugins.register(EchoPlugin);
    plugins.register(AddPlugin);
    plugins.register(WhoamiPlugin);
    plugins.register(GuessPlugin);
    plugins.register(UrlDetectorPlugin);
    plugins.register(ToolboxPlugin);
    plugins.register(QWeatherPlugin);
    let built = plugins.build();

    let dispatch_options = DispatchOptions::new(["/", "!", "."]);
    let dispatcher =
        Dispatcher::with_runtime_state(built, dispatch_options, runtime_ops.plugins.clone());
    let mut event_rx = OneBotV11Adapter::new(onebot_ws_url.clone()).start().await;
    let mut cron_tick = tokio::time::interval(Duration::from_secs(1));

    info!("Managed OneBot bot running on {}", onebot_ws_url);

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                info!("Managed bot loop received shutdown signal.");
                break;
            }
            _ = cron_tick.tick() => {
                if !runtime_ops.is_running().await {
                    continue;
                }
                if let Err(err) = runtime_ops.wasm.trigger_cron("*/1 * * * * *").await {
                    warn!("Wasm cron trigger error: {}", err);
                }
            }
            maybe_ctx = event_rx.recv() => {
                let Some(ctx) = maybe_ctx else {
                    info!("OneBot adapter channel closed.");
                    break;
                };

                metrics_store.upsert(MetricEvent {
                    bot_id: bot_id.clone(),
                    name: "events_in_total".to_string(),
                    value: 1,
                    labels: HashMap::new(),
                });

                if !runtime_ops.is_running().await {
                    continue;
                }

                let text = ctx.text();
                if let Err(err) = dispatcher.dispatch(&ctx).await {
                    metrics_store.upsert(MetricEvent {
                        bot_id: bot_id.clone(),
                        name: "plugin_errors_total".to_string(),
                        value: 1,
                        labels: HashMap::new(),
                    });
                    error!("Plugin dispatch error: {}", err);
                }

                if let Some(line) = ayiou::core::parse_command_line(&text, &["/", "!", "."])
                    && let Err(err) = runtime_ops
                        .wasm
                        .dispatch_command(line.command(), line.args())
                        .await
                {
                    warn!("Wasm command dispatch error: {}", err);
                }

                if let Err(err) = runtime_ops.wasm.dispatch_regex(&text).await {
                    warn!("Wasm regex dispatch error: {}", err);
                }
            }
        }
    }

    Ok(())
}
