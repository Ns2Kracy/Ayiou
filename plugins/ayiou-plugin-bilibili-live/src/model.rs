use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotifyTarget {
    Group(i64),
    Private(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TargetSubscriptions {
    pub uids: BTreeSet<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct StreamerState {
    pub uid: u64,
    pub room_id: Option<u64>,
    pub uname: Option<String>,
    pub is_live: bool,
    pub title: Option<String>,
    pub live_url: Option<String>,
    pub cover_url: Option<String>,
    pub last_live_started_at: Option<String>,
    pub last_seen_at: Option<String>,
}
