use anyhow::anyhow;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

// Message

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    String(String),
    Array(Vec<MessageSegment>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MessageSegment {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "face")]
    Face { id: String },
    #[serde(rename = "image")]
    Image {
        file: String,
        #[serde(rename = "type")]
        #[serde(skip_serializing_if = "Option::is_none")]
        image_type: Option<String>, // flash
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    #[serde(rename = "record")]
    Record {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        magic: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    #[serde(rename = "video")]
    Video {
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    #[serde(rename = "at")]
    At {
        qq: String, // "all" for all members
    },
    #[serde(rename = "rps")]
    Rps,
    #[serde(rename = "dice")]
    Dice,
    #[serde(rename = "shake")]
    Shake,
    #[serde(rename = "poke")]
    Poke {
        #[serde(rename = "type")]
        poke_type: String,
        id: String,
    },
    #[serde(rename = "anonymous")]
    Anonymous,
    #[serde(rename = "share")]
    Share {
        url: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    #[serde(rename = "contact")]
    Contact {
        #[serde(rename = "type")]
        contact_type: String, // "qq", "group"
        id: String,
    },
    #[serde(rename = "location")]
    Location {
        lat: String,
        lon: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
    },
    #[serde(rename = "music")]
    Music {
        #[serde(rename = "type")]
        music_type: String, // "qq", "163", "xm"
        id: String,
    },
    #[serde(rename = "reply")]
    Reply { id: String },
    #[serde(rename = "forward")]
    Forward { id: String },
    #[serde(rename = "node")]
    Node { id: String },
    #[serde(rename = "xml")]
    Xml { data: String },
    #[serde(rename = "json")]
    Json { data: String },
    #[serde(other)]
    Unknown,
}

impl MessageSegment {
    /// Write preview text into buffer to avoid extra allocations
    pub fn write_preview(&self, buf: &mut String) {
        use std::fmt::Write;
        match self {
            MessageSegment::Text { text } => buf.push_str(text),
            MessageSegment::Face { id } => {
                let _ = write!(buf, "[表情:{}]", id);
            }
            MessageSegment::Image { .. } => buf.push_str("[图片]"),
            MessageSegment::Record { .. } => buf.push_str("[语音]"),
            MessageSegment::Video { .. } => buf.push_str("[视频]"),
            MessageSegment::At { qq } => {
                let _ = write!(buf, "[@{}]", qq);
            }
            MessageSegment::Rps => buf.push_str("[猜拳]"),
            MessageSegment::Dice => buf.push_str("[骰子]"),
            MessageSegment::Shake => buf.push_str("[戳一戳]"),
            MessageSegment::Poke { .. } => buf.push_str("[戳一戳]"),
            MessageSegment::Anonymous => buf.push_str("[匿名]"),
            MessageSegment::Share { title, .. } => {
                let _ = write!(buf, "[分享:{}]", title);
            }
            MessageSegment::Contact { contact_type, id } => {
                let _ = write!(buf, "[推荐{}:{}]", contact_type, id);
            }
            MessageSegment::Location { .. } => buf.push_str("[位置]"),
            MessageSegment::Music { music_type, .. } => {
                let _ = write!(buf, "[音乐:{}]", music_type);
            }
            MessageSegment::Reply { id } => {
                let _ = write!(buf, "[回复:{}]", id);
            }
            MessageSegment::Forward { .. } => buf.push_str("[转发消息]"),
            MessageSegment::Node { .. } => buf.push_str("[消息节点]"),
            MessageSegment::Xml { .. } => buf.push_str("[XML消息]"),
            MessageSegment::Json { .. } => buf.push_str("[JSON消息]"),
            MessageSegment::Unknown => buf.push_str("[未知消息]"),
        }
    }
}

// Event

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "post_type")]
pub enum OneBotEvent {
    #[serde(rename = "message")]
    Message(Box<MessageEvent>),
    #[serde(rename = "notice")]
    Notice(NoticeEvent),
    #[serde(rename = "request")]
    Request(RequestEvent),
    #[serde(rename = "meta_event")]
    Meta(MetaEvent),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "message_type")]
pub enum MessageEvent {
    #[serde(rename = "private")]
    Private(Box<PrivateMessageEvent>),
    #[serde(rename = "group")]
    Group(Box<GroupMessageEvent>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrivateMessageEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "friend", "group", "other"
    pub message_id: i32,
    pub user_id: i64,
    pub message: Message,
    pub raw_message: String,
    pub font: i32,
    pub sender: PrivateSender,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupMessageEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "normal", "anonymous", "notice"
    pub message_id: i32,
    pub group_name: String,
    pub group_id: i64,
    pub user_id: i64,
    pub anonymous: Option<Anonymous>,
    pub message: Message,
    pub raw_message: String,
    pub font: i32,
    pub sender: GroupSender,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "notice_type")]
pub enum NoticeEvent {
    #[serde(rename = "group_upload")]
    GroupUpload(GroupUploadNoticeEvent),
    #[serde(rename = "group_admin")]
    GroupAdmin(GroupAdminNoticeEvent),
    #[serde(rename = "group_decrease")]
    GroupDecrease(GroupDecreaseNoticeEvent),
    #[serde(rename = "group_increase")]
    GroupIncrease(GroupIncreaseNoticeEvent),
    #[serde(rename = "group_ban")]
    GroupBan(GroupBanNoticeEvent),
    #[serde(rename = "friend_add")]
    FriendAdd(FriendAddNoticeEvent),
    #[serde(rename = "group_recall")]
    GroupRecall(GroupRecallNoticeEvent),
    #[serde(rename = "friend_recall")]
    FriendRecall(FriendRecallNoticeEvent),
    #[serde(rename = "group_card")]
    GroupCard(GroupCardNoticeEvent),
    #[serde(rename = "offline_file")]
    OfflineFile(OfflineFileNoticeEvent),
    #[serde(rename = "client_status")]
    ClientStatus(ClientStatusNoticeEvent),
    #[serde(rename = "essence")]
    Essence(EssenceNoticeEvent),
    #[serde(rename = "notify")]
    Notify(NotifyEvent),
    /// Unknown notice types (extensions like group_msg_emoji_like)
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupUploadNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub file: File,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupAdminNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "set", "unset"
    pub group_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupDecreaseNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "leave", "kick", "kick_me"
    pub group_id: i64,
    pub operator_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupIncreaseNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "approve", "invite"
    pub group_id: i64,
    pub operator_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupBanNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "ban", "lift_ban"
    pub group_id: i64,
    pub operator_id: i64,
    pub user_id: i64,
    pub duration: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FriendAddNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupRecallNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub operator_id: i64,
    pub message_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FriendRecallNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub user_id: i64,
    pub message_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "sub_type")]
pub enum NotifyEvent {
    #[serde(rename = "poke")]
    Poke(PokeNotifyEvent),
    #[serde(rename = "lucky_king")]
    LuckyKing(LuckyKingNotifyEvent),
    #[serde(rename = "honor")]
    Honor(HonorNotifyEvent),
    #[serde(rename = "title")]
    Title(TitleNotifyEvent),
    /// Unknown notify sub_types
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PokeNotifyEvent {
    pub time: i64,
    pub self_id: i64,
    pub group_id: Option<i64>,
    pub user_id: i64,
    pub target_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LuckyKingNotifyEvent {
    pub time: i64,
    pub self_id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub target_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HonorNotifyEvent {
    pub time: i64,
    pub self_id: i64,
    pub group_id: i64,
    pub honor_type: String, // "talkative", "performer", "emotion"
    pub user_id: i64,
}

/// Group title change notification
#[derive(Debug, Clone, Deserialize)]
pub struct TitleNotifyEvent {
    pub time: i64,
    pub self_id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub title: String,
}

/// Group card (nickname) change notification (extension)
#[derive(Debug, Clone, Deserialize)]
pub struct GroupCardNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub group_id: i64,
    pub user_id: i64,
    pub card_new: String,
    pub card_old: String,
}

/// Offline file received notification (extension)
#[derive(Debug, Clone, Deserialize)]
pub struct OfflineFileNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub user_id: i64,
    pub file: OfflineFile,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OfflineFile {
    pub name: String,
    pub size: i64,
    pub url: String,
}

/// Client status change notification (extension)
#[derive(Debug, Clone, Deserialize)]
pub struct ClientStatusNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub online: bool,
    pub client: ClientDevice,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientDevice {
    pub app_id: i64,
    pub device_name: String,
    pub device_kind: String,
}

/// Essence message notification (extension)
#[derive(Debug, Clone, Deserialize)]
pub struct EssenceNoticeEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "add", "delete"
    pub group_id: i64,
    pub sender_id: i64,
    pub operator_id: i64,
    pub message_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "request_type")]
pub enum RequestEvent {
    #[serde(rename = "friend")]
    Friend(FriendRequestEvent),
    #[serde(rename = "group")]
    Group(GroupRequestEvent),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FriendRequestEvent {
    pub time: i64,
    pub self_id: i64,
    pub user_id: i64,
    pub comment: String,
    pub flag: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupRequestEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "add", "invite"
    pub group_id: i64,
    pub user_id: i64,
    pub comment: String,
    pub flag: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "meta_event_type")]
pub enum MetaEvent {
    #[serde(rename = "lifecycle")]
    Lifecycle(LifecycleMetaEvent),
    #[serde(rename = "heartbeat")]
    Heartbeat(HeartbeatMetaEvent),
}

#[derive(Debug, Clone, Deserialize)]
pub struct LifecycleMetaEvent {
    pub time: i64,
    pub self_id: i64,
    pub sub_type: String, // "enable", "disable", "connect"
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatMetaEvent {
    pub time: i64,
    pub self_id: i64,
    pub status: HeartbeatStatus,
    pub interval: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatStatus {
    pub online: bool,
    pub good: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrivateSender {
    pub user_id: i64,
    pub nickname: String,
    pub sex: Option<String>, // "male", "female", "unknown"
    pub age: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupSender {
    pub user_id: i64,
    pub nickname: String,
    pub card: Option<String>,
    pub sex: Option<String>, // "male", "female", "unknown"
    pub age: Option<i32>,
    pub area: Option<String>,
    pub level: Option<String>,
    pub role: Option<String>, // "owner", "admin", "member"
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Anonymous {
    pub id: i64,
    pub name: String,
    pub flag: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct File {
    pub id: String,
    pub name: String,
    pub size: i64,
    pub busid: i64,
}

#[derive(Debug, Serialize)]
pub struct ApiRequest {
    pub action: String,
    pub params: serde_json::Value,
    pub echo: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse {
    pub status: String,
    pub retcode: i32,
    #[serde(default)]
    pub data: serde_json::Value,
    pub echo: Option<String>,
}

impl ApiResponse {
    pub fn is_ok(&self) -> bool {
        self.status == "ok" && self.retcode == 0
    }

    pub fn ensure_ok(&self, action: &str) -> anyhow::Result<()> {
        if self.is_ok() {
            Ok(())
        } else {
            Err(anyhow!(
                "OneBot action `{}` failed: status={}, retcode={}, data={}",
                action,
                self.status,
                self.retcode,
                self.data
            ))
        }
    }

    pub fn data_as<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        serde_json::from_value(self.data.clone())
            .map_err(|e| anyhow!("Failed to decode OneBot response data: {}", e))
    }

    pub fn data_as_checked<T: DeserializeOwned>(&self, action: &str) -> anyhow::Result<T> {
        self.ensure_ok(action)?;
        self.data_as()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageData {
    pub message_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginInfoData {
    pub user_id: i64,
    pub nickname: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupInfoData {
    pub group_id: i64,
    pub group_name: String,
    pub member_count: Option<i32>,
    pub max_member_count: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupMemberInfoData {
    pub group_id: i64,
    pub user_id: i64,
    pub nickname: String,
    pub card: Option<String>,
    pub sex: Option<String>,
    pub age: Option<i32>,
    pub area: Option<String>,
    pub join_time: Option<i64>,
    pub last_sent_time: Option<i64>,
    pub level: Option<String>,
    pub role: Option<String>,
    pub unfriendly: Option<bool>,
    pub title: Option<String>,
    pub title_expire_time: Option<i64>,
    pub card_changeable: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum OneBotAction {
    SendPrivateMsg {
        user_id: i64,
        message: Message,
    },
    SendGroupMsg {
        group_id: i64,
        message: Message,
    },
    SetGroupKick {
        group_id: i64,
        user_id: i64,
        reject_add_request: bool,
    },
    DeleteMsg {
        message_id: i32,
    },
    GetLoginInfo,
    GetGroupInfo {
        group_id: i64,
        no_cache: bool,
    },
    GetGroupMemberInfo {
        group_id: i64,
        user_id: i64,
        no_cache: bool,
    },
    SetGroupBan {
        group_id: i64,
        user_id: i64,
        duration: i64,
    },
}

impl OneBotAction {
    pub fn into_request(self) -> ApiRequest {
        use serde_json::json;

        match self {
            Self::SendPrivateMsg { user_id, message } => ApiRequest {
                action: "send_private_msg".to_string(),
                params: json!({
                    "user_id": user_id,
                    "message": message,
                }),
                echo: None,
            },
            Self::SendGroupMsg { group_id, message } => ApiRequest {
                action: "send_group_msg".to_string(),
                params: json!({
                    "group_id": group_id,
                    "message": message,
                }),
                echo: None,
            },
            Self::SetGroupKick {
                group_id,
                user_id,
                reject_add_request,
            } => ApiRequest {
                action: "set_group_kick".to_string(),
                params: json!({
                    "group_id": group_id,
                    "user_id": user_id,
                    "reject_add_request": reject_add_request,
                }),
                echo: None,
            },
            Self::DeleteMsg { message_id } => ApiRequest {
                action: "delete_msg".to_string(),
                params: json!({ "message_id": message_id }),
                echo: None,
            },
            Self::GetLoginInfo => ApiRequest {
                action: "get_login_info".to_string(),
                params: json!({}),
                echo: None,
            },
            Self::GetGroupInfo { group_id, no_cache } => ApiRequest {
                action: "get_group_info".to_string(),
                params: json!({
                    "group_id": group_id,
                    "no_cache": no_cache,
                }),
                echo: None,
            },
            Self::GetGroupMemberInfo {
                group_id,
                user_id,
                no_cache,
            } => ApiRequest {
                action: "get_group_member_info".to_string(),
                params: json!({
                    "group_id": group_id,
                    "user_id": user_id,
                    "no_cache": no_cache,
                }),
                echo: None,
            },
            Self::SetGroupBan {
                group_id,
                user_id,
                duration,
            } => ApiRequest {
                action: "set_group_ban".to_string(),
                params: json!({
                    "group_id": group_id,
                    "user_id": user_id,
                    "duration": duration,
                }),
                echo: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_response_check_ok_and_decode() {
        let resp = ApiResponse {
            status: "ok".to_string(),
            retcode: 0,
            data: serde_json::json!({ "user_id": 1, "nickname": "bot" }),
            echo: Some("x".to_string()),
        };

        let data: LoginInfoData = resp.data_as_checked("get_login_info").unwrap();
        assert_eq!(data.user_id, 1);
        assert_eq!(data.nickname, "bot");
    }

    #[test]
    fn api_response_check_err() {
        let resp = ApiResponse {
            status: "failed".to_string(),
            retcode: 1400,
            data: serde_json::json!({ "msg": "bad request" }),
            echo: None,
        };

        let err = resp.ensure_ok("send_group_msg").unwrap_err();
        assert!(err.to_string().contains("send_group_msg"));
    }
}
