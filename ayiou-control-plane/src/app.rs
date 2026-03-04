use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::State,
    Router,
    extract::Path,
    http::StatusCode,
    routing::{get, post, put},
};
use ayiou_admin_proto::{AdminCommand, ConfigBackend};
use serde::Deserialize;

use crate::{
    agent_session::{AgentSessionHandle, RecordingAgentSession},
    auth::AuthenticatedUser,
    bot_registry::BotRegistry,
    plugin_service::{ConfigStore, InMemoryConfigStore},
    rbac,
};

#[derive(Clone)]
pub struct AppState {
    users_by_token: Arc<HashMap<String, AuthenticatedUser>>,
    bot_registry: BotRegistry,
    config_store: Arc<dyn ConfigStore>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            users_by_token: Arc::new(HashMap::new()),
            bot_registry: BotRegistry::default(),
            config_store: Arc::new(InMemoryConfigStore::default()),
        }
    }
}

impl AppState {
    pub fn single_user(username: &str, token: &str, permissions: &[&str]) -> Self {
        let user = AuthenticatedUser::new(username, permissions);
        let mut users = HashMap::new();
        users.insert(token.to_string(), user);

        Self {
            users_by_token: Arc::new(users),
            bot_registry: BotRegistry::default(),
            config_store: Arc::new(InMemoryConfigStore::default()),
        }
    }

    pub fn user_for_token(&self, token: &str) -> Option<AuthenticatedUser> {
        self.users_by_token.get(token).cloned()
    }

    pub fn bot_registry(&self) -> &BotRegistry {
        &self.bot_registry
    }

    pub fn config_store(&self) -> Arc<dyn ConfigStore> {
        self.config_store.clone()
    }

    pub fn with_config_store(mut self, config_store: Arc<dyn ConfigStore>) -> Self {
        self.config_store = config_store;
        self
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/bots/{id}/start", post(start_bot))
        .route(
            "/api/v1/bots/{id}/plugins/{name}/disable",
            post(disable_plugin),
        )
        .route(
            "/api/v1/bots/{id}/plugins/{name}/config",
            put(update_plugin_config),
        )
        .with_state(state)
}

pub fn test_app_with_connected_agent(bot_id: &str) -> (Router, RecordingAgentSession) {
    let state = AppState::single_user("admin", "admin-token", &["bot:start", "plugin:disable"]);
    let recording = RecordingAgentSession::default();
    state
        .bot_registry()
        .register(bot_id.to_string(), AgentSessionHandle::new(recording.clone()));
    (build_router(state), recording)
}

pub fn test_app_with_store_and_agent() -> (Router, RecordingAgentSession, Arc<InMemoryConfigStore>)
{
    let store = Arc::new(InMemoryConfigStore::default());
    let state = AppState::single_user("admin", "admin-token", &["config:write"])
        .with_config_store(store.clone());
    let recording = RecordingAgentSession::default();

    state
        .bot_registry()
        .register("bot-a".to_string(), AgentSessionHandle::new(recording.clone()));

    (build_router(state), recording, store)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn start_bot(
    Path(bot_id): Path<String>,
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<StatusCode, StatusCode> {
    rbac::require(&user, "bot:start")?;
    state
        .bot_registry()
        .send_command(&bot_id, AdminCommand::StartBot)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(StatusCode::ACCEPTED)
}

async fn disable_plugin(
    Path((bot_id, plugin_name)): Path<(String, String)>,
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<StatusCode, StatusCode> {
    rbac::require(&user, "plugin:disable")?;
    state
        .bot_registry()
        .send_command(&bot_id, AdminCommand::DisablePlugin { plugin_name })
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Deserialize)]
struct UpdatePluginConfigRequest {
    backend: ConfigBackend,
    content: String,
    expected_version: Option<u64>,
}

async fn update_plugin_config(
    Path((bot_id, plugin_name)): Path<(String, String)>,
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(request): Json<UpdatePluginConfigRequest>,
) -> Result<StatusCode, StatusCode> {
    rbac::require(&user, "config:write")?;
    let version = state
        .config_store()
        .put(
            &bot_id,
            &plugin_name,
            request.backend.clone(),
            &request.content,
            request.expected_version,
        )
        .await
        .map_err(|err| {
            if err.to_string().contains("version conflict") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    state
        .bot_registry()
        .send_command(
            &bot_id,
            AdminCommand::UpdatePluginConfig {
                plugin_name,
                backend: request.backend,
                content: request.content,
                expected_version: Some(version),
            },
        )
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(StatusCode::ACCEPTED)
}
