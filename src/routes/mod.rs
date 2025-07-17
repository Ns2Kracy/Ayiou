use axum::Router;

use crate::Ctx;

pub mod api;

pub fn mount() -> Router<Ctx> {
    Router::new().nest(
        "/api",
        Router::new()
            .merge(auth_routes())
            .merge(user_routes())
            .merge(link_routes())
            .merge(user_link_routes())
            .merge(analytics_routes()),
    )
}

fn auth_routes() -> Router<Ctx> {
    Router::new().nest("/auth", api::auth::mount())
}

fn link_routes() -> Router<Ctx> {
    Router::new().nest("/links", Router::new())
}

fn user_routes() -> Router<Ctx> {
    Router::new().nest("/users", api::user::mount())
}

fn user_link_routes() -> Router<Ctx> {
    Router::new().nest("/user-links", api::user_link::mount())
}

fn analytics_routes() -> Router<Ctx> {
    Router::new().nest("/analytics", Router::new())
}

fn redirect_routes() -> Router<Ctx> {
    Router::new()
}
