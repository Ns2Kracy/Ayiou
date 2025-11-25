use serde::{Deserialize, Serialize};

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
    Private(PrivateMessageEvent),
    #[serde(rename = "group")]
    Group(GroupMessageEvent),
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
    #[serde(rename = "notify")]
    Notify(NotifyEvent),
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
