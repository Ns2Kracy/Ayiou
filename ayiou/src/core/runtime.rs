use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RuntimeState {
    Running,
    #[default]
    Stopped,
}

#[derive(Clone, Default)]
pub struct RuntimeController {
    state: Arc<RwLock<RuntimeState>>,
}

impl RuntimeController {
    pub fn new(initial: RuntimeState) -> Self {
        Self {
            state: Arc::new(RwLock::new(initial)),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = RuntimeState::Running;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.write().await;
        *state = RuntimeState::Stopped;
        Ok(())
    }

    pub async fn state(&self) -> RuntimeState {
        *self.state.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_state_transitions_are_idempotent() {
        let rt = RuntimeController::default();
        assert!(rt.start().await.is_ok());
        assert!(rt.start().await.is_ok());
        assert!(rt.stop().await.is_ok());
        assert!(rt.stop().await.is_ok());
        assert_eq!(rt.state().await, RuntimeState::Stopped);
    }
}
