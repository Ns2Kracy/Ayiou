use std::{env, path::PathBuf, sync::Arc};

use axum::{Extension, Router, http::HeaderName, middleware};
use ayiou::{Context, app::config::ConfigManager};
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    propagate_header::PropagateHeaderLayer,
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    services::ServeDir,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load configuration
    let config_path = env::var("AYIAH_CONFIG_PATH").map(PathBuf::from).ok();

    // Initialize config manager
    let config_manager = ConfigManager::init(config_path)?;

    // Create application router
    let app = Router::new()
        // .merge(routes::mount())
        .layer(Extension(Arc::new(Context {
            // db: conn,
            config: config_manager.clone(),
        })))
        .layer(middleware::from_fn(middleware_logger))
        .layer(CompressionLayer::new())
        .layer(PropagateHeaderLayer::new(HeaderName::from_static(
            "x-request-id",
        )))
        .layer(SetRequestIdLayer::new(
            HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(CorsLayer::permissive());

    // Parse host:port string into SocketAddr

    let address = config_manager.socket_addr()?;

    let listener = TcpListener::bind(address).await?;

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
