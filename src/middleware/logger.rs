use std::time::Instant;

use axum::{
    body::Body,
    http::{Request, Response},
    middleware::Next,
};
use tracing::info;

/// Custom request logger middleware
pub async fn logger(request: Request<Body>, next: Next) -> Response<Body> {
    // Extract request information and create owned copies of everything
    let method = request.method().clone();
    let uri_path = request.uri().path().to_string();
    let version = request.version();
    let request_id = request
        .headers()
        .get("x-request-id")
        .map(|v| v.to_str().unwrap_or("-"))
        .unwrap_or("-")
        .to_string();

    // Get user agent
    let user_agent = request
        .headers()
        .get("user-agent")
        .map(|v| v.to_str().unwrap_or("-"))
        .unwrap_or("-")
        .to_string();

    // Record start time for latency calculation
    let start = Instant::now();

    // Process the request
    let response = next.run(request).await;

    // Calculate latency
    let latency = start.elapsed();

    // Get response status
    let status = response.status().as_u16();

    // Log using structured format
    info!(
        method = %method,
        uri = %uri_path,
        ?version,
        status = %status,
        ?latency,
        user_agent = %user_agent,
        request_id = ?request_id,
    );

    response
}
