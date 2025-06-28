use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, http::HeaderName, middleware};
use ayiou::{
    Context,
    app::config::CONFIG,
    middleware::logger::logger,
    routes,
    utils::{self, graceful_shutdown::shutdown_signal},
};
use moka::future::Cache;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    propagate_header::PropagateHeaderLayer,
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    timeout::TimeoutLayer,
};
use tracing::error;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config = &*CONFIG;

    utils::logger::init(&config.logging).map_err(|e| anyhow::anyhow!(e))?;

    // Initialize database connection pool
    let db_pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .acquire_timeout(Duration::from_secs(config.database.connect_timeout))
        .connect(&config.database.database_url())
        .await
        .map_err(|e| {
            error!("Failed to connect to database: {}", e);
            e
        })?;

    // Run database migrations
    sqlx::migrate!("./migrations")
        .run(&db_pool)
        .await
        .map_err(|e| {
            error!("Failed to run migrations: {}", e);
            e
        })?;

    // Create application router
    let app = Router::new()
        .merge(routes::mount())
        .with_state(Arc::new(Context {
            db: db_pool,
            cache: Cache::builder()
                .max_capacity(config.cache.max_entries)
                .time_to_live(Duration::from_secs(config.cache.ttl_seconds))
                .build(),
        }))
        .layer(middleware::from_fn(logger))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(PropagateHeaderLayer::new(HeaderName::from_static(
            "x-request-id",
        )))
        .layer(SetRequestIdLayer::new(
            HeaderName::from_static("x-request-id"),
            MakeRequestUuid,
        ))
        .layer(CorsLayer::permissive());

    // Parse host:port string into SocketAddr
    let addr = format!("{}:{}", config.server.host, config.server.port)
        .parse::<SocketAddr>()
        .expect("Invalid server address configuration");

    let listener = TcpListener::bind(addr).await?;

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
