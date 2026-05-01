use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RuntimeState {
    Starting,
    Running,
    Stopping,
    #[default]
    Stopped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub last_error: Option<String>,
}

#[derive(Clone, Default)]
pub struct RuntimeController {
    status: Arc<RwLock<RuntimeStatus>>,
}

impl RuntimeController {
    #[must_use]
    pub fn new(initial: RuntimeState) -> Self {
        Self {
            status: Arc::new(RwLock::new(RuntimeStatus {
                state: initial,
                last_error: None,
            })),
        }
    }

    pub async fn start(&self) -> Result<()> {
        {
            let mut status = self.status.write().await;
            status.state = RuntimeState::Running;
            status.last_error = None;
        }
        Ok(())
    }

    pub async fn mark_starting(&self) -> Result<()> {
        {
            let mut status = self.status.write().await;
            status.state = RuntimeState::Starting;
        }
        Ok(())
    }

    pub async fn mark_stopping(&self) -> Result<()> {
        {
            let mut status = self.status.write().await;
            status.state = RuntimeState::Stopping;
        }
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        {
            let mut status = self.status.write().await;
            status.state = RuntimeState::Stopped;
        }
        Ok(())
    }

    pub async fn fail(&self, error: impl Into<String>) {
        let mut status = self.status.write().await;
        status.state = RuntimeState::Failed;
        status.last_error = Some(error.into());
    }

    pub async fn state(&self) -> RuntimeState {
        self.status.read().await.state
    }

    pub async fn status(&self) -> RuntimeStatus {
        self.status.read().await.clone()
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

    #[tokio::test]
    async fn runtime_status_records_failures() {
        let rt = RuntimeController::default();
        rt.fail("boom").await;
        let status = rt.status().await;
        assert_eq!(status.state, RuntimeState::Failed);
        assert_eq!(status.last_error.as_deref(), Some("boom"));
    }
}
