use serde::{Deserialize, Serialize};
use serde_json::Value;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "post_type")]
pub enum OneBotEvent {
    #[serde(rename = "meta_event")]
    MetaEvent(MetaEvent),
    #[serde(rename = "message")]
    Message(MessageEvent),
    #[serde(rename = "notice")]
    Notice(Value),
    #[serde(rename = "request")]
    Request(Value),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MetaEvent {
    pub meta_event_type: String,
    pub self_id: i64,
    // ... heartbeat fields
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct MessageEvent {
    pub message_type: String, // private, group
    pub sub_type: Option<String>,
    pub message_id: i32,
    pub user_id: i64,
    pub group_id: Option<i64>,
    // message can be string or array in OneBot. For simplicity, we mostly rely on raw_message.
    pub raw_message: String,
    pub self_id: i64,
}

#[derive(Debug, Serialize)]
pub struct ApiRequest {
    pub action: String,
    pub params: Value,
    pub echo: Option<String>,
}
