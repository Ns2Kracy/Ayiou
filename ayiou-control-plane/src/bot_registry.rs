use anyhow::{Result, anyhow};
use ayiou_admin_proto::{AdminCommand, CommandEnvelope};
use dashmap::DashMap;

use crate::agent_session::AgentSessionHandle;

#[derive(Clone, Default)]
pub struct BotRegistry {
    sessions: std::sync::Arc<DashMap<String, AgentSessionHandle>>,
}

impl BotRegistry {
    pub fn register(&self, bot_id: impl Into<String>, session: AgentSessionHandle) {
        self.sessions.insert(bot_id.into(), session);
    }

    pub async fn send_command(&self, bot_id: &str, command: AdminCommand) -> Result<()> {
        let envelope = CommandEnvelope::new(uuid::Uuid::new_v4().to_string(), bot_id, command);
        let session = self
            .sessions
            .get(bot_id)
            .ok_or_else(|| anyhow!("agent offline: {}", bot_id))?;

        session.send(envelope).await
    }

    pub fn list_bot_ids(&self) -> Vec<String> {
        let mut bots: Vec<_> = self
            .sessions
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        bots.sort();
        bots
    }
}
