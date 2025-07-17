use axum::Router;

use crate::Ctx;

pub mod api;

pub fn mount() -> Router<Ctx> {
    Router::new()
        .nest("/api", api_routes())
        .merge(public_routes())
}

fn api_routes() -> Router<Ctx> {
    Router::new()
        .merge(auth_routes())
        .merge(link_routes())
        .merge(user_routes())
        .merge(user_link_routes())
        .merge(analytics_routes())
}

fn public_routes() -> Router<Ctx> {
    Router::new()
        .merge(redirect_routes())
        .merge(user_page_routes())
}

fn auth_routes() -> Router<Ctx> {
    Router::new().nest("/auth", Router::new())
}

fn link_routes() -> Router<Ctx> {
    Router::new().nest("/links", Router::new())
}

fn user_routes() -> Router<Ctx> {
    Router::new().nest("/users", Router::new())
}

fn user_link_routes() -> Router<Ctx> {
    Router::new().nest("/links", api::user_link::routes())
}

fn analytics_routes() -> Router<Ctx> {
    Router::new().nest("/analytics", Router::new())
}

fn redirect_routes() -> Router<Ctx> {
    Router::new()
}

fn user_page_routes() -> Router<Ctx> {
    // User page routes: ayiou.com/{username}
    api::user_link::public_routes()
}
