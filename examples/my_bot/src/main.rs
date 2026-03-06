use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use ayiou::{
    NoopWasmHost, RuntimeController, RuntimeState, WasmRuntime,
    adapter::onebot::v11::{adapter::OneBotV11Adapter, ctx::Ctx},
    core::{DispatchOptions, Dispatcher, PluginManager, PluginRuntimeState, adapter::Adapter},
};
use ayiou_plugin_qweather::QWeatherPlugin;
use log::{error, info, warn};
use tokio::sync::Notify;

mod plugin;

use plugin::{AddPlugin, EchoPlugin, GuessPlugin, ToolboxPlugin, UrlDetectorPlugin, WhoamiPlugin};

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

    let onebot_ws_url =
        std::env::var("ONEBOT_WS_URL").unwrap_or_else(|_| "ws://192.168.31.180:3001".to_string());

    let runtime_ops = MyBotRuntimeOps {
        runtime: RuntimeController::default(),
        plugins: PluginRuntimeState::default(),
        wasm: WasmRuntime::new(NoopWasmHost),
    };

    info!("Running my_bot in standalone mode");
    info!("OneBot endpoint: {}", onebot_ws_url);
    info!("Sample wasm plugin path: examples/my_bot/wasm/wtest_plugin.wasm");

    let shutdown = Arc::new(Notify::new());
    let bot_shutdown = shutdown.clone();
    let runtime_for_bot = runtime_ops.clone();
    let onebot_for_bot = onebot_ws_url.clone();

    let bot_task = tokio::spawn(async move {
        run_managed_onebot_bot(runtime_for_bot, onebot_for_bot, bot_shutdown).await
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
    shutdown: Arc<Notify>,
) -> Result<()> {
    runtime_ops.runtime.start().await?;

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

    Ok(())
}
