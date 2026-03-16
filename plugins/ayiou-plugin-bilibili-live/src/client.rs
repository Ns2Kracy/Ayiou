use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveStreamer {
    pub uid: u64,
    pub uname: String,
    pub room_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveStatus {
    pub room_id: u64,
    pub is_live: bool,
    pub title: String,
    pub live_url: String,
    pub cover_url: Option<String>,
    pub live_started_at: Option<String>,
}

#[async_trait]
pub trait LiveClient: Send + Sync {
    async fn resolve_streamer(&self, uid: u64) -> Result<ResolveStreamer>;
    async fn fetch_live_status(&self, room_id: u64) -> Result<LiveStatus>;
}

#[derive(Clone, Default)]
pub struct BilibiliLiveClient {
    http: Client,
}

impl BilibiliLiveClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }
}

#[async_trait]
impl LiveClient for BilibiliLiveClient {
    async fn resolve_streamer(&self, uid: u64) -> Result<ResolveStreamer> {
        let payload: Value = self
            .http
            .get("https://api.live.bilibili.com/live_user/v1/Master/info")
            .query(&[("uid", uid)])
            .send()
            .await
            .context("failed to call bilibili master info")?
            .error_for_status()
            .context("bilibili master info returned non-2xx status")?
            .json()
            .await
            .context("failed to decode bilibili master info response")?;

        ensure_success(&payload)?;

        let data = payload
            .get("data")
            .ok_or_else(|| anyhow!("bilibili master info missing data"))?;
        let info = data
            .get("info")
            .ok_or_else(|| anyhow!("bilibili master info missing info"))?;
        let uname = get_string(info, "uname")?;
        let room_id = get_u64(data, "room_id")?;

        if room_id == 0 {
            bail!("bilibili streamer {} has no live room", uid);
        }

        Ok(ResolveStreamer { uid, uname, room_id })
    }

    async fn fetch_live_status(&self, room_id: u64) -> Result<LiveStatus> {
        let payload: Value = self
            .http
            .get("https://api.live.bilibili.com/room/v1/Room/get_info")
            .query(&[("id", room_id)])
            .send()
            .await
            .context("failed to call bilibili room info")?
            .error_for_status()
            .context("bilibili room info returned non-2xx status")?
            .json()
            .await
            .context("failed to decode bilibili room info response")?;

        ensure_success(&payload)?;

        let data = payload
            .get("data")
            .ok_or_else(|| anyhow!("bilibili room info missing data"))?;
        let is_live = get_u64(data, "live_status").unwrap_or(0) > 0;
        let title = get_optional_string(data, "title").unwrap_or_default();
        let cover_url = get_optional_string(data, "user_cover")
            .or_else(|| get_optional_string(data, "cover"));
        let live_started_at = get_optional_string(data, "live_time")
            .filter(|value| !value.is_empty() && value != "0000-00-00 00:00:00");

        Ok(LiveStatus {
            room_id,
            is_live,
            title,
            live_url: format!("https://live.bilibili.com/{}", room_id),
            cover_url,
            live_started_at,
        })
    }
}

fn ensure_success(payload: &Value) -> Result<()> {
    let code = payload
        .get("code")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow!("bilibili response missing code"))?;
    if code == 0 {
        Ok(())
    } else {
        bail!("bilibili api returned code {}", code)
    }
}

fn get_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("missing string field `{}`", key))
}

fn get_optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(ToString::to_string)
}

fn get_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(|field| {
            field
                .as_u64()
                .or_else(|| field.as_i64().and_then(|num| u64::try_from(num).ok()))
                .or_else(|| field.as_str().and_then(|raw| raw.parse::<u64>().ok()))
        })
        .ok_or_else(|| anyhow!("missing numeric field `{}`", key))
}
