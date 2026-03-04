use axum::{
    Router,
    extract::{Path, State},
    response::Html,
    routing::get,
};

use crate::{app::AppState, auth::AuthenticatedUser};

pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/ui/bots", get(bots_page))
        .route("/ui/bots/{id}", get(bot_detail_page))
}

async fn bots_page(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Html<String>, axum::http::StatusCode> {
    let mut bots = state.known_bots();
    bots.sort();

    let mut items = String::new();
    for bot in bots {
        items.push_str(&format!(r#"<li><a href="/ui/bots/{bot}">{bot}</a></li>"#));
    }

    let html = format!(
        r#"
<!doctype html>
<html>
  <head><title>Ayiou Control Plane</title></head>
  <body>
    <h1>Bots</h1>
    <ul>{items}</ul>
  </body>
</html>
"#
    );

    Ok(Html(html))
}

async fn bot_detail_page(
    Path(bot_id): Path<String>,
    _state: State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Html<String>, axum::http::StatusCode> {
    let html = format!(
        r#"
<!doctype html>
<html>
  <head><title>Bot {bot_id}</title></head>
  <body>
    <h1>Bot: {bot_id}</h1>
    <button>Start Bot</button>
    <button>Stop Bot</button>
    <button>Enable Plugin</button>
    <button>Disable Plugin</button>
    <form>
      <label>Load Wasm Plugin</label>
      <input name="module_path" type="text" placeholder="/path/to/plugin.wasm"/>
      <button type="submit">Load Wasm Plugin</button>
    </form>
    <button>Unload Wasm Plugin</button>
    <form>
      <label>Plugin Config</label>
      <textarea name="config" rows="5" cols="60"></textarea>
      <button type="submit">Save Config</button>
    </form>
  </body>
</html>
"#
    );
    Ok(Html(html))
}
