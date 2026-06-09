use std::sync::Arc;

use anyhow::Result;
use tokio::sync::RwLock;

use crate::core::plugin::{RuntimePluginEngine, RuntimePluginSnapshot};

pub struct RuntimeControlHandle {
    engine: Arc<RwLock<RuntimePluginEngine>>,
}

impl Clone for RuntimeControlHandle {
    fn clone(&self) -> Self {
        Self {
            engine: self.engine.clone(),
        }
    }
}

impl RuntimeControlHandle {
    #[must_use]
    pub const fn new(engine: Arc<RwLock<RuntimePluginEngine>>) -> Self {
        Self { engine }
    }

    pub async fn plugin_snapshots(&self) -> Vec<RuntimePluginSnapshot> {
        self.engine.read().await.plugin_snapshots()
    }

    pub async fn enable_plugin(&self, instance_id: &str) -> Result<()> {
        self.engine.write().await.enable_plugin(instance_id).await
    }

    pub async fn disable_plugin(&self, instance_id: &str) -> Result<()> {
        self.engine.write().await.disable_plugin(instance_id).await
    }

    pub async fn start_plugin(&self, instance_id: &str) -> Result<()> {
        self.engine.write().await.start_plugin(instance_id).await
    }

    pub async fn stop_plugin(&self, instance_id: &str) -> Result<()> {
        self.engine.write().await.stop_plugin(instance_id).await
    }

    pub async fn reload_plugin(&self, instance_id: &str) -> Result<()> {
        self.engine.write().await.reload_plugin(instance_id).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use async_trait::async_trait;

    use crate::core::{
        context::Context,
        plugin::{
            HandleOutcome, HandlerDecl, PluginRuntimeState, RuntimePlugin, RuntimePluginEngine,
            RuntimePluginServices,
        },
    };

    use super::*;

    struct ControlPlugin {
        stopped: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl RuntimePlugin for ControlPlugin {
        fn kind(&self) -> &'static str {
            "control-plugin"
        }
        fn declared_handlers(&self) -> Vec<HandlerDecl> {
            vec![HandlerDecl::wildcard_message()]
        }

        async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
            Ok(HandleOutcome::pass())
        }

        async fn stop(&mut self) -> Result<()> {
            *self.stopped.lock().unwrap() += 1;
            Ok(())
        }
    }

    fn test_handle(state: PluginRuntimeState, stopped: Arc<Mutex<usize>>) -> RuntimeControlHandle {
        let services = RuntimePluginServices::new();
        let mut engine = RuntimePluginEngine::new(services, state);
        engine.push_as("control-plugin", Box::new(ControlPlugin { stopped }));
        RuntimeControlHandle::new(Arc::new(RwLock::new(engine)))
    }

    #[tokio::test]
    async fn runtime_control_handle_reports_plugin_snapshots() {
        let state = PluginRuntimeState::default();
        let handle = test_handle(state, Arc::new(Mutex::new(0)));

        let snapshots = handle.plugin_snapshots().await;

        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].instance_id, "control-plugin");
    }

    #[tokio::test]
    async fn runtime_control_handle_disables_plugins() {
        let state = PluginRuntimeState::default();
        let stopped = Arc::new(Mutex::new(0));
        let handle = test_handle(state.clone(), stopped.clone());

        handle.start_plugin("control-plugin").await.unwrap();
        handle.disable_plugin("control-plugin").await.unwrap();

        assert!(!state.is_enabled("control-plugin"));
        assert_eq!(*stopped.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn runtime_control_handle_reports_non_reloadable_plugins() {
        let state = PluginRuntimeState::default();
        let handle = test_handle(state, Arc::new(Mutex::new(0)));

        let err = handle.reload_plugin("control-plugin").await.unwrap_err();

        assert!(err.to_string().contains("not reloadable"));
    }
}
