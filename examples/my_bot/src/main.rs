use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use ayiou::{
    NoopWasmHost, RuntimeController, RuntimeState, WasmRuntime,
    adapter::onebot::v11::{adapter::OneBotV11Adapter, ctx::Ctx},
    core::{
        DispatchOptions, Dispatcher, PluginManager, PluginRuntimeState, adapter::Adapter,
        plugin_host::PluginHost,
        scheduler::{Scheduler, TokioScheduler},
        storage::{SeaOrmStore, Store},
    },
    driver::wsclient::WsDriver,
};
use ayiou_plugin_bilibili_live::BilibiliLivePlugin;
use ayiou_plugin_qweather::QWeatherPlugin;
use log::{error, info, warn};
use tokio::sync::Notify;

mod plugin;

use plugin::{EchoPlugin, GuessPlugin, ToolboxPlugin, WhoamiPlugin};

#[derive(Clone, Default)]
struct MyBotRuntimeOps {
    runtime: RuntimeController,
    plugins: PluginRuntimeState,
    wasm: WasmRuntime,
}

impl MyBotRuntimeOps {
    pub async fn is_running(&self) -> bool {
        self.runtime.state().await == RuntimeState::Running
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    unsafe {
        std::env::set_var("RUST_LOG", "DEBUG");
    }

    pretty_env_logger::try_init().ok();

    let onebot_ws_url = onebot_ws_url_from_env();
    let onebot_access_token = onebot_access_token_from_env();
    let onebot_display_url = redact_access_token_in_url(&onebot_ws_url);

    let runtime_ops = MyBotRuntimeOps {
        runtime: RuntimeController::default(),
        plugins: PluginRuntimeState::default(),
        wasm: WasmRuntime::new(NoopWasmHost),
    };

    info!("Running my_bot in standalone mode");
    info!("OneBot endpoint: {}", onebot_display_url);
    info!("Sample wasm plugin path: examples/my_bot/wasm/wtest_plugin.wasm");

    let shutdown = Arc::new(Notify::new());
    let bot_shutdown = shutdown.clone();
    let runtime_for_bot = runtime_ops.clone();
    let onebot_for_bot = onebot_ws_url.clone();
    let token_for_bot = onebot_access_token.clone();

    let bot_task = tokio::spawn(async move {
        run_managed_onebot_bot(runtime_for_bot, onebot_for_bot, token_for_bot, bot_shutdown).await
    });

    let mut bot_task = bot_task;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down my_bot...");
            shutdown.notify_waiters();
        }
        result = &mut bot_task => {
            let bot_result = result.context("join bot task")?;
            bot_result?;
            return Ok(());
        }
    }

    let bot_result = bot_task.await.context("join bot task")?;
    bot_result?;
    Ok(())
}

async fn run_managed_onebot_bot(
    runtime_ops: MyBotRuntimeOps,
    onebot_ws_url: String,
    onebot_access_token: Option<String>,
    shutdown: Arc<Notify>,
) -> Result<()> {
    runtime_ops.runtime.start().await?;
    let store: Arc<dyn Store> = Arc::new(SeaOrmStore::connect(&database_url_from_env()).await?);
    let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());

    let mut plugins = PluginManager::<Ctx>::new();
    plugins.register(EchoPlugin);
    plugins.register(WhoamiPlugin);
    plugins.register(GuessPlugin);
    plugins.register(ToolboxPlugin);
    plugins.register(BilibiliLivePlugin::new());
    plugins.register(QWeatherPlugin);
    let built = plugins.build();

    let token_auth_enabled =
        onebot_ws_url.contains("access_token=") || onebot_access_token.is_some();
    let adapter = if let Some(token) = onebot_access_token {
        info!("OneBot token auth: enabled");
        OneBotV11Adapter::with_token(onebot_ws_url.clone(), token)
    } else {
        info!(
            "OneBot token auth: {}",
            if token_auth_enabled { "enabled" } else { "disabled" }
        );
        OneBotV11Adapter::new(onebot_ws_url.clone())
    };

    let adapter_runtime = adapter.start_with_runtime().await;
    let plugin_host = PluginHost::new(
        scheduler.clone(),
        store,
        adapter_runtime.sender.clone(),
    );
    for plugin in built.iter() {
        plugin.start(plugin_host.clone()).await?;
    }

    let dispatch_options = DispatchOptions::new(["/", "!", "."]);
    let dispatcher =
        Dispatcher::with_runtime_state(built, dispatch_options, runtime_ops.plugins.clone());
    let mut event_rx = adapter_runtime.events;
    let mut cron_tick = tokio::time::interval(Duration::from_secs(1));

    info!(
        "Managed OneBot bot running on {}",
        redact_access_token_in_url(&onebot_ws_url)
    );
    info!("Bilibili live plugin uses database persistence");

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
                match runtime_ops.wasm.trigger_cron("*/1 * * * * *").await {
                    Ok(true) => info!("[wasm] handled cron tick"),
                    Ok(false) => {}
                    Err(err) => warn!("Wasm cron trigger error: {}", err),
                }
            }
            maybe_ctx = event_rx.recv() => {
                let Some(ctx) = maybe_ctx else {
                    info!("OneBot adapter channel closed.");
                    break;
                };

                if !runtime_ops.is_running().await {
                    continue;
                }

                let text = ctx.text();
                if let Err(err) = dispatcher.dispatch(&ctx).await {
                    error!("Plugin dispatch error: {}", err);
                }

                if let Some(line) = ayiou::core::parse_command_line(&text, &["/", "!", "."])
                {
                    match runtime_ops
                        .wasm
                        .dispatch_command_calls(line.command(), line.args())
                        .await
                    {
                        Ok(calls) => {
                            if !calls.is_empty() {
                                info!(
                                    "[wasm] handled command '{}' with args '{}'",
                                    line.command(),
                                    line.args()
                                );
                            }
                            for call in calls {
                                if let Some(reply_text) = call.reply_text()
                                    && let Err(err) = ctx.reply_text(reply_text.to_string()).await
                                {
                                    warn!("Wasm command reply error: {}", err);
                                }
                            }
                        }
                        Err(err) => warn!("Wasm command dispatch error: {}", err),
                    }
                }

                match runtime_ops.wasm.dispatch_regex_calls(&text).await {
                    Ok(calls) => {
                        if !calls.is_empty() {
                            info!("[wasm] handled regex text: {}", text);
                        }
                        for call in calls {
                            if let Some(reply_text) = call.reply_text()
                                && let Err(err) = ctx.reply_text(reply_text.to_string()).await
                            {
                                warn!("Wasm regex reply error: {}", err);
                            }
                        }
                    }
                    Err(err) => warn!("Wasm regex dispatch error: {}", err),
                }
            }
        }
    }

    scheduler.shutdown().await?;
    Ok(())
}

fn database_url_from_env() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://my_bot.sqlite?mode=rwc".to_string())
}

fn onebot_ws_url_from_env() -> String {
    std::env::var("ONEBOT_WS_URL").unwrap_or_else(|_| "ws://192.168.31.180:3001".to_string())
}

fn onebot_access_token_from_env() -> Option<String> {
    std::env::var("ONEBOT_ACCESS_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn redact_access_token_in_url(url: &str) -> String {
    WsDriver::new(url).redacted_url()
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::{
        database_url_from_env, onebot_access_token_from_env, onebot_ws_url_from_env,
        redact_access_token_in_url,
    };

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn database_url_defaults_to_sqlite_file() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("DATABASE_URL");
        }

        assert_eq!(database_url_from_env(), "sqlite://my_bot.sqlite?mode=rwc");
    }

    #[test]
    fn database_url_prefers_env_override() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("DATABASE_URL", "sqlite://custom.sqlite?mode=rwc");
        }

        assert_eq!(database_url_from_env(), "sqlite://custom.sqlite?mode=rwc");

        unsafe {
            std::env::remove_var("DATABASE_URL");
        }
    }

    #[test]
    fn access_token_from_env_treats_empty_as_missing() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("ONEBOT_ACCESS_TOKEN", "");
        }

        assert_eq!(onebot_access_token_from_env(), None);

        unsafe {
            std::env::remove_var("ONEBOT_ACCESS_TOKEN");
        }
    }

    #[test]
    fn access_token_from_env_reads_non_empty_value() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("ONEBOT_ACCESS_TOKEN", "secret");
        }

        assert_eq!(onebot_access_token_from_env(), Some("secret".to_string()));

        unsafe {
            std::env::remove_var("ONEBOT_ACCESS_TOKEN");
        }
    }

    #[test]
    fn ws_url_from_env_prefers_url_query_token() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var(
                "ONEBOT_WS_URL",
                "ws://127.0.0.1:3001/?access_token=url-token",
            );
            std::env::set_var("ONEBOT_ACCESS_TOKEN", "env-token");
        }

        assert_eq!(
            onebot_ws_url_from_env(),
            "ws://127.0.0.1:3001/?access_token=url-token"
        );

        unsafe {
            std::env::remove_var("ONEBOT_WS_URL");
            std::env::remove_var("ONEBOT_ACCESS_TOKEN");
        }
    }

    #[test]
    fn redact_access_token_in_url_hides_query_secret() {
        assert_eq!(
            redact_access_token_in_url("ws://127.0.0.1:3001/?access_token=secret-token"),
            "ws://127.0.0.1:3001/?access_token=***"
        );
    }
}
