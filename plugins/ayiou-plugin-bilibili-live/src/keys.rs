pub const TARGET_GROUP_PREFIX: &str = "plugin:blive:target:group:";
pub const TARGET_PRIVATE_PREFIX: &str = "plugin:blive:target:private:";
pub const STREAMER_PREFIX: &str = "plugin:blive:streamer:";

pub fn group_target_key(group_id: i64) -> String {
    format!("{}{}", TARGET_GROUP_PREFIX, group_id)
}

pub fn private_target_key(user_id: i64) -> String {
    format!("{}{}", TARGET_PRIVATE_PREFIX, user_id)
}

pub fn streamer_state_key(uid: u64) -> String {
    format!("{}{}", STREAMER_PREFIX, uid)
}

pub fn parse_group_target_key(key: &str) -> Option<i64> {
    key.strip_prefix(TARGET_GROUP_PREFIX)?.parse().ok()
}

pub fn parse_private_target_key(key: &str) -> Option<i64> {
    key.strip_prefix(TARGET_PRIVATE_PREFIX)?.parse().ok()
}
