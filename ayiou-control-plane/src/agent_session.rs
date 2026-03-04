use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use ayiou_admin_proto::CommandEnvelope;
use tokio::sync::Mutex;

#[async_trait]
pub trait AgentSession: Send + Sync {
    async fn send(&self, command: CommandEnvelope) -> Result<()>;
}

#[derive(Clone)]
pub struct AgentSessionHandle {
    inner: Arc<dyn AgentSession>,
}

impl AgentSessionHandle {
    pub fn new<T>(inner: T) -> Self
    where
        T: AgentSession + 'static,
    {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn send(&self, command: CommandEnvelope) -> Result<()> {
        self.inner.send(command).await
    }
}

#[derive(Clone, Default)]
pub struct RecordingAgentSession {
    last_command: Arc<Mutex<Option<CommandEnvelope>>>,
}

#[async_trait]
impl AgentSession for RecordingAgentSession {
    async fn send(&self, command: CommandEnvelope) -> Result<()> {
        *self.last_command.lock().await = Some(command);
        Ok(())
    }
}

impl RecordingAgentSession {
    pub async fn last_command(&self) -> CommandEnvelope {
        self.last_command
            .lock()
            .await
            .clone()
            .expect("recorded command should exist")
    }
}
