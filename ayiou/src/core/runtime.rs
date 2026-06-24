use std::sync::Arc;

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

    pub async fn start(&self) {
        self.transition(RuntimeState::Running, true).await;
    }

    pub async fn mark_starting(&self) {
        self.transition(RuntimeState::Starting, false).await;
    }

    pub async fn mark_stopping(&self) {
        self.transition(RuntimeState::Stopping, false).await;
    }

    pub async fn stop(&self) {
        self.transition(RuntimeState::Stopped, false).await;
    }

    pub async fn fail(&self, error: impl Into<String>) {
        let mut status = self.status.write().await;
        status.state = RuntimeState::Failed;
        status.last_error = Some(error.into());
    }

    async fn transition(&self, state: RuntimeState, clear_error: bool) {
        let mut status = self.status.write().await;
        status.state = state;
        if clear_error {
            status.last_error = None;
        }
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
        rt.start().await;
        rt.start().await;
        rt.stop().await;
        rt.stop().await;
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
