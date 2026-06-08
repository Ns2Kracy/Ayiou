use std::{net::SocketAddr, sync::Arc};

use anyhow::{Result, anyhow};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
#[cfg(feature = "embedded-webui")]
use axum::{body::Body, http::Uri};
#[cfg(feature = "embedded-webui")]
use include_dir::{Dir, include_dir};
use serde::Serialize;
use serde_json::json;
use tokio::task::JoinHandle;

#[cfg(feature = "embedded-webui")]
static WEBUI_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../webui/dist");

use crate::core::{
    control::RuntimeControlHandle,
    plugin_runtime::PluginInstanceState,
    plugin_system::{Capability, PluginHealth, PluginMetadata, RuntimePluginManifest},
    service::ServiceKey,
};

#[derive(Clone, Debug)]
pub struct ControlPlaneOptions {
    bind: String,
    token: Option<String>,
}

impl ControlPlaneOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn bind(mut self, bind: impl Into<String>) -> Self {
        self.bind = bind.into();
        self
    }

    #[must_use]
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn bind_addr(&self) -> Result<SocketAddr> {
        self.bind
            .parse()
            .map_err(|err| anyhow!("invalid control plane bind address `{}`: {err}", self.bind))
    }

    #[must_use]
    pub fn token_value(&self) -> Option<&str> {
        self.token.as_deref()
    }
}

impl Default for ControlPlaneOptions {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:32187".to_string(),
            token: None,
        }
    }
}

struct ControlPlaneState<C> {
    handle: RuntimeControlHandle<C>,
    token: Arc<str>,
}

impl<C> Clone for ControlPlaneState<C> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            token: self.token.clone(),
        }
    }
}

pub fn router<C>(handle: RuntimeControlHandle<C>, token: impl Into<String>) -> Router
where
    C: Send + Sync + 'static,
{
    let state = ControlPlaneState {
        handle,
        token: Arc::from(token.into()),
    };

    let router = Router::new()
        .route("/api/runtime", get(runtime))
        .route("/api/plugins", get(list_plugins))
        .route("/api/plugins/{id}", get(get_plugin))
        .route("/api/plugins/{id}/enable", post(enable_plugin))
        .route("/api/plugins/{id}/disable", post(disable_plugin))
        .route("/api/plugins/{id}/start", post(start_plugin))
        .route("/api/plugins/{id}/stop", post(stop_plugin))
        .route("/api/plugins/{id}/reload", post(reload_plugin))
        .with_state(state);

    #[cfg(feature = "embedded-webui")]
    let router = router.route("/", get(index_asset)).fallback(static_asset);

    router
}

pub fn spawn<C>(
    options: ControlPlaneOptions,
    handle: RuntimeControlHandle<C>,
) -> Result<JoinHandle<()>>
where
    C: Send + Sync + 'static,
{
    let token = options
        .token_value()
        .ok_or_else(|| anyhow!("control plane token is required"))?
        .to_string();
    let bind = options.bind_addr()?;
    let app = router(handle, token);

    Ok(tokio::spawn(async move {
        match tokio::net::TcpListener::bind(bind).await {
            Ok(listener) => {
                if let Err(err) = axum::serve(listener, app).await {
                    log::error!("Control plane server failed: {err}");
                }
            }
            Err(err) => log::error!("Control plane failed to bind {bind}: {err}"),
        }
    }))
}

async fn runtime<C>(State(state): State<ControlPlaneState<C>>, headers: HeaderMap) -> Response
where
    C: Send + Sync + 'static,
{
    if let Err(response) = authorize(&headers, &state.token) {
        return response;
    }

    let plugins = state.handle.plugin_snapshots().await;
    ok(json!({
        "plugin_count": plugins.len(),
    }))
}

async fn list_plugins<C>(State(state): State<ControlPlaneState<C>>, headers: HeaderMap) -> Response
where
    C: Send + Sync + 'static,
{
    if let Err(response) = authorize(&headers, &state.token) {
        return response;
    }

    let plugins: Vec<_> = state
        .handle
        .plugin_snapshots()
        .await
        .into_iter()
        .map(PluginSnapshotDto::from)
        .collect();
    ok(plugins)
}

async fn get_plugin<C>(
    State(state): State<ControlPlaneState<C>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response
where
    C: Send + Sync + 'static,
{
    if let Err(response) = authorize(&headers, &state.token) {
        return response;
    }

    let plugin = state
        .handle
        .plugin_snapshots()
        .await
        .into_iter()
        .find(|plugin| plugin.instance_id == id)
        .map(PluginSnapshotDto::from);

    match plugin {
        Some(plugin) => ok(plugin),
        None => api_error(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("plugin `{id}` was not found"),
        ),
    }
}

async fn enable_plugin<C>(
    State(state): State<ControlPlaneState<C>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response
where
    C: Send + Sync + 'static,
{
    plugin_action(headers, &state, &id, PluginAction::Enable).await
}

async fn disable_plugin<C>(
    State(state): State<ControlPlaneState<C>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response
where
    C: Send + Sync + 'static,
{
    plugin_action(headers, &state, &id, PluginAction::Disable).await
}

async fn start_plugin<C>(
    State(state): State<ControlPlaneState<C>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response
where
    C: Send + Sync + 'static,
{
    plugin_action(headers, &state, &id, PluginAction::Start).await
}

async fn stop_plugin<C>(
    State(state): State<ControlPlaneState<C>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response
where
    C: Send + Sync + 'static,
{
    plugin_action(headers, &state, &id, PluginAction::Stop).await
}

async fn reload_plugin<C>(
    State(state): State<ControlPlaneState<C>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response
where
    C: Send + Sync + 'static,
{
    plugin_action(headers, &state, &id, PluginAction::Reload).await
}

#[derive(Clone, Copy)]
enum PluginAction {
    Enable,
    Disable,
    Start,
    Stop,
    Reload,
}

async fn plugin_action<C>(
    headers: HeaderMap,
    state: &ControlPlaneState<C>,
    id: &str,
    action: PluginAction,
) -> Response
where
    C: Send + Sync + 'static,
{
    if let Err(response) = authorize(&headers, &state.token) {
        return response;
    }

    let result = match action {
        PluginAction::Enable => state.handle.enable_plugin(id).await,
        PluginAction::Disable => state.handle.disable_plugin(id).await,
        PluginAction::Start => state.handle.start_plugin(id).await,
        PluginAction::Stop => state.handle.stop_plugin(id).await,
        PluginAction::Reload => state.handle.reload_plugin(id).await,
    };

    match result {
        Ok(()) => ok(json!({ "instance_id": id })),
        Err(err) => {
            let message = err.to_string();
            let code = if message.contains("not reloadable") {
                "not_reloadable"
            } else if message.contains("not registered") {
                "not_found"
            } else {
                "operation_failed"
            };
            let status = if code == "not_found" {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            api_error(status, code, message)
        }
    }
}

fn authorize(headers: &HeaderMap, token: &str) -> std::result::Result<(), Response> {
    let expected = format!("Bearer {token}");
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);

    if authorized {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "valid bearer token required",
        ))
    }
}

fn ok(data: impl Serialize) -> Response {
    Json(json!({
        "ok": true,
        "data": data,
    }))
    .into_response()
}

fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({
            "ok": false,
            "error": {
                "code": code,
                "message": message.into(),
            },
        })),
    )
        .into_response()
}

#[derive(Serialize)]
struct PluginSnapshotDto {
    instance_id: String,
    kind: String,
    meta: PluginMetadataDto,
    manifest: RuntimePluginManifestDto,
    lifecycle: PluginInstanceStateDto,
    health: PluginHealthDto,
    reloadable: bool,
}

impl From<crate::core::plugin_system::RuntimePluginSnapshot> for PluginSnapshotDto {
    fn from(snapshot: crate::core::plugin_system::RuntimePluginSnapshot) -> Self {
        Self {
            instance_id: snapshot.instance_id,
            kind: snapshot.kind,
            meta: snapshot.meta.into(),
            manifest: snapshot.manifest.into(),
            lifecycle: snapshot.lifecycle.into(),
            health: snapshot.health.into(),
            reloadable: false,
        }
    }
}

#[derive(Serialize)]
struct PluginMetadataDto {
    name: String,
    description: String,
    version: String,
}

impl From<PluginMetadata> for PluginMetadataDto {
    fn from(meta: PluginMetadata) -> Self {
        Self {
            name: meta.name,
            description: meta.description,
            version: meta.version,
        }
    }
}

#[derive(Serialize)]
struct RuntimePluginManifestDto {
    kind: String,
    description: String,
    version: String,
    required_capabilities: Vec<String>,
    optional_capabilities: Vec<String>,
    required_services: Vec<String>,
    optional_services: Vec<String>,
}

impl From<RuntimePluginManifest> for RuntimePluginManifestDto {
    fn from(manifest: RuntimePluginManifest) -> Self {
        Self {
            kind: manifest.kind,
            description: manifest.description,
            version: manifest.version,
            required_capabilities: manifest
                .required_capabilities
                .into_iter()
                .map(capability_name)
                .collect(),
            optional_capabilities: manifest
                .optional_capabilities
                .into_iter()
                .map(capability_name)
                .collect(),
            required_services: manifest
                .required_services
                .iter()
                .map(service_key_name)
                .collect(),
            optional_services: manifest
                .optional_services
                .iter()
                .map(service_key_name)
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct PluginInstanceStateDto {
    enabled: bool,
    desired_config_version: u64,
    applied_config_version: u64,
    config_lifecycle_state: String,
    lifecycle_state: String,
    last_error: Option<String>,
}

impl From<PluginInstanceState> for PluginInstanceStateDto {
    fn from(state: PluginInstanceState) -> Self {
        Self {
            enabled: state.enabled,
            desired_config_version: state.desired_config_version,
            applied_config_version: state.applied_config_version,
            config_lifecycle_state: format!("{:?}", state.config_lifecycle_state),
            lifecycle_state: format!("{:?}", state.lifecycle_state),
            last_error: state.last_error,
        }
    }
}

#[derive(Serialize)]
struct PluginHealthDto {
    healthy: bool,
    detail: Option<String>,
}

impl From<PluginHealth> for PluginHealthDto {
    fn from(health: PluginHealth) -> Self {
        Self {
            healthy: health.healthy,
            detail: health.detail,
        }
    }
}

fn capability_name(capability: Capability) -> String {
    match capability {
        Capability::ProactiveSend => "proactive_send".to_string(),
        Capability::MessageDelete => "message_delete".to_string(),
        Capability::Reaction => "reaction".to_string(),
        Capability::GroupModeration => "group_moderation".to_string(),
        Capability::RichSegments => "rich_segments".to_string(),
        Capability::Custom(name) => name,
    }
}

fn service_key_name(key: &ServiceKey) -> String {
    key.type_name().to_string()
}

#[cfg(feature = "embedded-webui")]
async fn index_asset() -> Response {
    match WEBUI_DIST.get_file("index.html") {
        Some(file) => Html(String::from_utf8_lossy(file.contents()).into_owned()).into_response(),
        None => api_error(
            StatusCode::NOT_FOUND,
            "asset_not_found",
            "embedded web UI index was not found",
        ),
    }
}

#[cfg(feature = "embedded-webui")]
async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return index_asset().await;
    }

    match WEBUI_DIST.get_file(path) {
        Some(file) => {
            let content_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            (
                [(header::CONTENT_TYPE, content_type)],
                Body::from(file.contents().to_vec()),
            )
                .into_response()
        }
        None => index_asset().await,
    }
}
