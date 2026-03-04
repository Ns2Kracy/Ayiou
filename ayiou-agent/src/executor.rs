use anyhow::Result;
use ayiou_admin_proto::{AdminCommand, CommandEnvelope, ConfigBackend};
use dashmap::DashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAckStatus {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandAck {
    pub command_id: String,
    pub status: CommandAckStatus,
}

impl CommandAck {
    pub fn applied(command_id: impl Into<String>) -> Self {
        Self {
            command_id: command_id.into(),
            status: CommandAckStatus::Applied,
        }
    }

    pub fn already_applied(command_id: impl Into<String>) -> Self {
        Self {
            command_id: command_id.into(),
            status: CommandAckStatus::AlreadyApplied,
        }
    }
}

#[async_trait::async_trait]
pub trait RuntimeOps: Send + Sync {
    async fn start_bot(&self, bot_id: &str) -> Result<()>;
    async fn stop_bot(&self, bot_id: &str) -> Result<()>;
    async fn set_plugin_enabled(
        &self,
        bot_id: &str,
        plugin_name: &str,
        enabled: bool,
    ) -> Result<()>;
    async fn update_plugin_config(
        &self,
        bot_id: &str,
        plugin_name: &str,
        backend: ConfigBackend,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<()>;
    async fn load_wasm_plugin(
        &self,
        bot_id: &str,
        plugin_name: &str,
        module_path: &str,
    ) -> Result<()>;
    async fn unload_wasm_plugin(&self, bot_id: &str, plugin_name: &str) -> Result<()>;
}

pub struct CommandExecutor<R> {
    runtime: R,
    seen: DashSet<String>,
}

impl<R> CommandExecutor<R> {
    pub fn new(runtime: R) -> Self {
        Self {
            runtime,
            seen: DashSet::new(),
        }
    }
}

impl<R> CommandExecutor<R>
where
    R: RuntimeOps,
{
    pub async fn execute(&self, env: CommandEnvelope) -> Result<CommandAck> {
        if !self.seen.insert(env.command_id.clone()) {
            return Ok(CommandAck::already_applied(env.command_id));
        }

        match env.command {
            AdminCommand::StartBot => self.runtime.start_bot(&env.bot_id).await?,
            AdminCommand::StopBot => self.runtime.stop_bot(&env.bot_id).await?,
            AdminCommand::EnablePlugin { plugin_name } => {
                self.runtime
                    .set_plugin_enabled(&env.bot_id, &plugin_name, true)
                    .await?
            }
            AdminCommand::DisablePlugin { plugin_name } => {
                self.runtime
                    .set_plugin_enabled(&env.bot_id, &plugin_name, false)
                    .await?
            }
            AdminCommand::UpdatePluginConfig {
                plugin_name,
                backend,
                content,
                expected_version,
            } => {
                self.runtime
                    .update_plugin_config(
                        &env.bot_id,
                        &plugin_name,
                        backend,
                        &content,
                        expected_version,
                    )
                    .await?
            }
            AdminCommand::LoadWasmPlugin {
                plugin_name,
                module_path,
            } => {
                self.runtime
                    .load_wasm_plugin(&env.bot_id, &plugin_name, &module_path)
                    .await?
            }
            AdminCommand::UnloadWasmPlugin { plugin_name } => {
                self.runtime
                    .unload_wasm_plugin(&env.bot_id, &plugin_name)
                    .await?
            }
        }

        Ok(CommandAck::applied(env.command_id))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use tokio::sync::Mutex;

    use super::*;

    #[derive(Clone, Default)]
    struct FakeRuntime {
        starts: Arc<AtomicUsize>,
        calls: Arc<Mutex<Vec<String>>>,
    }

    impl FakeRuntime {
        fn start_calls(&self) -> usize {
            self.starts.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl RuntimeOps for FakeRuntime {
        async fn start_bot(&self, bot_id: &str) -> Result<()> {
            self.starts.fetch_add(1, Ordering::SeqCst);
            self.calls.lock().await.push(format!("start:{bot_id}"));
            Ok(())
        }

        async fn stop_bot(&self, bot_id: &str) -> Result<()> {
            self.calls.lock().await.push(format!("stop:{bot_id}"));
            Ok(())
        }

        async fn set_plugin_enabled(
            &self,
            bot_id: &str,
            plugin_name: &str,
            enabled: bool,
        ) -> Result<()> {
            self.calls
                .lock()
                .await
                .push(format!("plugin:{bot_id}:{plugin_name}:{enabled}"));
            Ok(())
        }

        async fn update_plugin_config(
            &self,
            bot_id: &str,
            plugin_name: &str,
            backend: ConfigBackend,
            content: &str,
            expected_version: Option<u64>,
        ) -> Result<()> {
            self.calls.lock().await.push(format!(
                "config:{bot_id}:{plugin_name}:{backend:?}:{content}:{}",
                expected_version.map(|v| v.to_string()).unwrap_or_default()
            ));
            Ok(())
        }

        async fn load_wasm_plugin(
            &self,
            bot_id: &str,
            plugin_name: &str,
            module_path: &str,
        ) -> Result<()> {
            self.calls
                .lock()
                .await
                .push(format!("wasm_load:{bot_id}:{plugin_name}:{module_path}"));
            Ok(())
        }

        async fn unload_wasm_plugin(&self, bot_id: &str, plugin_name: &str) -> Result<()> {
            self.calls
                .lock()
                .await
                .push(format!("wasm_unload:{bot_id}:{plugin_name}"));
            Ok(())
        }
    }

    #[tokio::test]
    async fn duplicate_command_id_is_executed_once() {
        let runtime = FakeRuntime::default();
        let exec = CommandExecutor::new(runtime.clone());

        let cmd = CommandEnvelope::new("c1", "bot-a", AdminCommand::StartBot);
        exec.execute(cmd.clone()).await.unwrap();
        exec.execute(cmd).await.unwrap();

        assert_eq!(runtime.start_calls(), 1);
    }

    #[tokio::test]
    async fn wasm_commands_are_forwarded_to_runtime_ops() {
        let runtime = FakeRuntime::default();
        let exec = CommandExecutor::new(runtime.clone());

        exec.execute(CommandEnvelope::new(
            "c2",
            "bot-a",
            AdminCommand::LoadWasmPlugin {
                plugin_name: "echo".into(),
                module_path: "/tmp/echo.wasm".into(),
            },
        ))
        .await
        .unwrap();

        exec.execute(CommandEnvelope::new(
            "c3",
            "bot-a",
            AdminCommand::UnloadWasmPlugin {
                plugin_name: "echo".into(),
            },
        ))
        .await
        .unwrap();

        let calls = runtime.calls.lock().await.clone();
        assert!(
            calls
                .iter()
                .any(|entry| entry == "wasm_load:bot-a:echo:/tmp/echo.wasm")
        );
        assert!(calls.iter().any(|entry| entry == "wasm_unload:bot-a:echo"));
    }
}
