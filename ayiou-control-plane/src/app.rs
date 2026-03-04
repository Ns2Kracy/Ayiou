use std::{collections::HashMap, sync::Arc};

use axum::{
    Router,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};

use crate::{auth::AuthenticatedUser, rbac};

#[derive(Clone, Default)]
pub struct AppState {
    users_by_token: Arc<HashMap<String, AuthenticatedUser>>,
}

impl AppState {
    pub fn single_user(username: &str, token: &str, permissions: &[&str]) -> Self {
        let user = AuthenticatedUser::new(username, permissions);
        let mut users = HashMap::new();
        users.insert(token.to_string(), user);

        Self {
            users_by_token: Arc::new(users),
        }
    }

    pub fn user_for_token(&self, token: &str) -> Option<AuthenticatedUser> {
        self.users_by_token.get(token).cloned()
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route(
            "/api/v1/bots/{id}/plugins/{name}/disable",
            post(disable_plugin),
        )
        .with_state(state)
}

async fn healthz() -> StatusCode {
    StatusCode::OK
}

async fn disable_plugin(
    Path((_bot_id, _plugin_name)): Path<(String, String)>,
    user: AuthenticatedUser,
) -> Result<StatusCode, StatusCode> {
    rbac::require(&user, "plugin:disable")?;
    Ok(StatusCode::ACCEPTED)
}
