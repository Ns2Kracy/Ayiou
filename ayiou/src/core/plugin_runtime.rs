use std::sync::Arc;

use dashmap::DashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PluginLifecycleState {
    #[default]
    Registered,
    Initializing,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginInstanceState {
    pub enabled: bool,
    pub desired_config_version: u64,
    pub applied_config_version: u64,
    pub lifecycle_state: PluginLifecycleState,
    pub last_error: Option<String>,
}

impl Default for PluginInstanceState {
    fn default() -> Self {
        Self {
            enabled: true,
            desired_config_version: 0,
            applied_config_version: 0,
            lifecycle_state: PluginLifecycleState::Registered,
            last_error: None,
        }
    }
}

#[derive(Default, Clone)]
pub struct PluginRuntimeState {
    instances: Arc<DashMap<String, PluginInstanceState>>,
}

impl PluginRuntimeState {
    fn update(&self, plugin: &str, f: impl FnOnce(&mut PluginInstanceState)) {
        let mut entry = self.instances.entry(plugin.to_string()).or_default();
        f(entry.value_mut());
    }

    pub fn snapshot(&self, plugin: &str) -> PluginInstanceState {
        self.instances
            .get(plugin)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    pub fn snapshots(&self) -> Vec<(String, PluginInstanceState)> {
        let mut snapshots: Vec<_> = self
            .instances
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
        snapshots.sort_by(|a, b| a.0.cmp(&b.0));
        snapshots
    }

    pub fn set_enabled(&self, plugin: &str, on: bool) {
        self.update(plugin, |state| state.enabled = on);
    }

    pub fn is_enabled(&self, plugin: &str) -> bool {
        self.snapshot(plugin).enabled
    }

    pub fn set_desired_config_version(&self, plugin: &str, version: u64) {
        self.update(plugin, |state| state.desired_config_version = version);
    }

    pub fn mark_config_applied(&self, plugin: &str, version: u64) {
        self.update(plugin, |state| {
            state.applied_config_version = version;
            state.desired_config_version = state.desired_config_version.max(version);
        });
    }

    pub fn set_lifecycle(&self, plugin: &str, lifecycle_state: PluginLifecycleState) {
        self.update(plugin, |state| state.lifecycle_state = lifecycle_state);
    }

    pub fn record_error(&self, plugin: &str, error: impl Into<String>) {
        self.update(plugin, |state| {
            state.lifecycle_state = PluginLifecycleState::Failed;
            state.last_error = Some(error.into());
        });
    }

    pub fn clear_error(&self, plugin: &str) {
        self.update(plugin, |state| state.last_error = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_state_tracks_versions_and_errors() {
        let state = PluginRuntimeState::default();
        state.set_enabled("echo", false);
        state.set_desired_config_version("echo", 3);
        state.mark_config_applied("echo", 2);
        state.record_error("echo", "boom");

        let snapshot = state.snapshot("echo");
        assert!(!snapshot.enabled);
        assert_eq!(snapshot.desired_config_version, 3);
        assert_eq!(snapshot.applied_config_version, 2);
        assert_eq!(snapshot.lifecycle_state, PluginLifecycleState::Failed);
        assert_eq!(snapshot.last_error.as_deref(), Some("boom"));
    }
}
