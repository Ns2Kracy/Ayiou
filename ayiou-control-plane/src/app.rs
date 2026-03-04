use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::State,
    Router,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use ayiou_admin_proto::AdminCommand;

use crate::{
    agent_session::{AgentSessionHandle, RecordingAgentSession},
    auth::AuthenticatedUser,
    bot_registry::BotRegistry,
    rbac,
};

#[derive(Clone, Default)]
pub struct AppState {
    users_by_token: Arc<HashMap<String, AuthenticatedUser>>,
    bot_registry: BotRegistry,
}

impl AppState {
    pub fn single_user(username: &str, token: &str, permissions: &[&str]) -> Self {
        let user = AuthenticatedUser::new(username, permissions);
        let mut users = HashMap::new();
        users.insert(token.to_string(), user);

        Self {
            users_by_token: Arc::new(users),
            bot_registry: BotRegistry::default(),
        }
    }

    pub fn user_for_token(&self, token: &str) -> Option<AuthenticatedUser> {
        self.users_by_token.get(token).cloned()
    }

    pub fn bot_registry(&self) -> &BotRegistry {
        &self.bot_registry
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
