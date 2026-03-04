use axum::{
    Router,
    extract::{Form, Path, State},
    http::{StatusCode, header::SET_COOKIE},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;

use crate::{app::AppState, auth::AuthenticatedUser};

const AUTH_COOKIE_NAME: &str = "ayiou_token";

#[derive(Debug, Deserialize)]
struct LoginForm {
    token: String,
}

pub fn ui_router() -> Router<AppState> {
    Router::new()
        .route("/ui/login", get(login_page).post(login_submit))
        .route("/ui/logout", post(logout_submit))
        .route("/ui/bots", get(bots_page))
        .route("/ui/bots/{id}", get(bot_detail_page))
}

async fn login_page() -> Html<String> {
    Html(render_login_page(None))
}

async fn login_submit(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let token = form.token.trim();
    if token.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            Html(render_login_page(Some("Token is required."))),
        )
            .into_response();
    }

    if state.user_for_token(token).is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Html(render_login_page(Some("Invalid token."))),
        )
            .into_response();
    }

    let cookie = format!(
        "{}={}; HttpOnly; Path=/; SameSite=Lax",
        AUTH_COOKIE_NAME, token
    );
    ([(SET_COOKIE, cookie)], Redirect::to("/ui/bots")).into_response()
}

async fn logout_submit() -> Response {
    let cookie = format!(
        "{}=; HttpOnly; Path=/; Max-Age=0; SameSite=Lax",
        AUTH_COOKIE_NAME
    );
    ([(SET_COOKIE, cookie)], Redirect::to("/ui/login")).into_response()
}

async fn bots_page(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Html<String>, StatusCode> {
    let mut bots = state.known_bots();
    bots.sort();

    let mut items = String::new();
    for bot in bots {
        let bot_escaped = escape_html(&bot);
        items.push_str(&format!(
            r#"<li><a href="/ui/bots/{bot_escaped}">{bot_escaped}</a></li>"#
        ));
    }

    let html = format!(
        r#"
<!doctype html>
<html>
  <head><title>Ayiou Control Plane</title></head>
  <body>
    <h1>Bots</h1>
    <form method="post" action="/ui/logout">
      <button type="submit">Logout</button>
    </form>
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
) -> Result<Html<String>, StatusCode> {
    let bot_escaped = escape_html(&bot_id);
    let html = r#"
<!doctype html>
<html>
  <head>
    <title>Bot __BOT_ID__</title>
    <style>
      body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; max-width: 980px; margin: 24px auto; line-height: 1.4; }
      section { border: 1px solid #ddd; border-radius: 8px; padding: 12px; margin-bottom: 12px; }
      label { display: block; font-weight: 600; margin-bottom: 6px; }
      input, select, textarea { width: 100%; box-sizing: border-box; margin-bottom: 8px; padding: 6px; }
      button { margin-right: 8px; margin-bottom: 8px; }
      pre { background: #111; color: #ddd; padding: 10px; border-radius: 6px; overflow-x: auto; }
    </style>
  </head>
  <body data-bot-id="__BOT_ID__">
    <h1>Bot: __BOT_ID__</h1>
    <form method="post" action="/ui/logout">
      <button type="submit">Logout</button>
    </form>

    <section>
      <button id="startBotBtn" type="button">Start Bot</button>
      <button id="stopBotBtn" type="button">Stop Bot</button>
    </section>

    <section>
      <label for="pluginName">Plugin Name</label>
      <input id="pluginName" type="text" value="echo" />
      <button id="enablePluginBtn" type="button">Enable Plugin</button>
      <button id="disablePluginBtn" type="button">Disable Plugin</button>
    </section>

    <section>
      <label for="modulePath">Load Wasm Plugin</label>
      <input id="modulePath" type="text" placeholder="/path/to/plugin.wasm" />
      <button id="loadWasmBtn" type="button">Load Wasm Plugin</button>
      <button id="unloadWasmBtn" type="button">Unload Wasm Plugin</button>
    </section>

    <section>
      <label for="configBackend">Config Backend</label>
      <select id="configBackend">
        <option value="toml">toml</option>
        <option value="sqlite">sqlite</option>
        <option value="postgres">postgres</option>
        <option value="redis">redis</option>
      </select>
      <label for="expectedVersion">Expected Version (optional)</label>
      <input id="expectedVersion" type="text" placeholder="leave empty for latest" />
      <label for="configContent">Plugin Config</label>
      <textarea id="configContent" rows="6" placeholder="threshold=3"></textarea>
      <button id="saveConfigBtn" type="button">Save Config</button>
    </section>

    <pre id="result">Ready.</pre>

    <script>
      const botId = document.body.dataset.botId;
      const result = document.getElementById("result");
      const pluginName = document.getElementById("pluginName");
      const modulePath = document.getElementById("modulePath");
      const configBackend = document.getElementById("configBackend");
      const expectedVersion = document.getElementById("expectedVersion");
      const configContent = document.getElementById("configContent");

      function setStatus(text) {
        result.textContent = text;
      }

      function pluginOrThrow() {
        const name = pluginName.value.trim();
        if (!name) {
          throw new Error("Plugin name is required");
        }
        return encodeURIComponent(name);
      }

      async function callApi(method, path, payload) {
        const headers = {};
        if (payload !== undefined) {
          headers["Content-Type"] = "application/json";
        }

        const response = await fetch(path, {
          method,
          headers,
          body: payload === undefined ? undefined : JSON.stringify(payload),
        });

        const text = await response.text();
        setStatus(`${method} ${path}\n${response.status} ${response.statusText}\n${text}`);
      }

      document.getElementById("startBotBtn").addEventListener("click", async () => {
        try {
          await callApi("POST", `/api/v1/bots/${encodeURIComponent(botId)}/start`);
        } catch (error) {
          setStatus(error.message);
        }
      });

      document.getElementById("stopBotBtn").addEventListener("click", async () => {
        try {
          await callApi("POST", `/api/v1/bots/${encodeURIComponent(botId)}/stop`);
        } catch (error) {
          setStatus(error.message);
        }
      });

      document.getElementById("enablePluginBtn").addEventListener("click", async () => {
        try {
          const plugin = pluginOrThrow();
          await callApi("POST", `/api/v1/bots/${encodeURIComponent(botId)}/plugins/${plugin}/enable`);
        } catch (error) {
          setStatus(error.message);
        }
      });

      document.getElementById("disablePluginBtn").addEventListener("click", async () => {
        try {
          const plugin = pluginOrThrow();
          await callApi("POST", `/api/v1/bots/${encodeURIComponent(botId)}/plugins/${plugin}/disable`);
        } catch (error) {
          setStatus(error.message);
        }
      });

      document.getElementById("loadWasmBtn").addEventListener("click", async () => {
        try {
          const plugin = pluginOrThrow();
          const path = modulePath.value.trim();
          if (!path) {
            throw new Error("Wasm module path is required");
          }
          await callApi("POST", `/api/v1/bots/${encodeURIComponent(botId)}/plugins/${plugin}/wasm/load`, {
            module_path: path,
          });
        } catch (error) {
          setStatus(error.message);
        }
      });

      document.getElementById("unloadWasmBtn").addEventListener("click", async () => {
        try {
          const plugin = pluginOrThrow();
          await callApi("POST", `/api/v1/bots/${encodeURIComponent(botId)}/plugins/${plugin}/wasm/unload`);
        } catch (error) {
          setStatus(error.message);
        }
      });

      document.getElementById("saveConfigBtn").addEventListener("click", async () => {
        try {
          const plugin = pluginOrThrow();
          const rawVersion = expectedVersion.value.trim();
          let parsedVersion = null;
          if (rawVersion !== "") {
            parsedVersion = Number(rawVersion);
            if (!Number.isInteger(parsedVersion) || parsedVersion < 0) {
              throw new Error("Expected version must be a non-negative integer");
            }
          }
          await callApi("PUT", `/api/v1/bots/${encodeURIComponent(botId)}/plugins/${plugin}/config`, {
            backend: configBackend.value,
            content: configContent.value,
            expected_version: parsedVersion,
          });
        } catch (error) {
          setStatus(error.message);
        }
      });
    </script>
  </body>
</html>
"#
    .replace("__BOT_ID__", &bot_escaped);

    Ok(Html(html))
}

fn render_login_page(error: Option<&str>) -> String {
    let error_html = error
        .map(|text| format!(r#"<p style="color:#b00020;">{}</p>"#, escape_html(text)))
        .unwrap_or_default();

    format!(
        r#"
<!doctype html>
<html>
  <head><title>Ayiou Login</title></head>
  <body>
    <h1>Sign In with Token</h1>
    {error_html}
    <form method="post" action="/ui/login">
      <label for="token">Token</label>
      <input id="token" name="token" type="password" />
      <button type="submit">Sign In</button>
    </form>
  </body>
</html>
"#
    )
}

fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
