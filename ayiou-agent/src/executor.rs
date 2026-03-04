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
    async fn set_plugin_enabled(&self, bot_id: &str, plugin_name: &str, enabled: bool)
    -> Result<()>;
    async fn update_plugin_config(
        &self,
        bot_id: &str,
        plugin_name: &str,
        backend: ConfigBackend,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<()>;
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
}
