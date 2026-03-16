use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use ayiou::core::storage::{Store, StoreSerdeExt};

use crate::keys::{
    TARGET_GROUP_PREFIX, TARGET_PRIVATE_PREFIX, group_target_key, parse_group_target_key,
    parse_private_target_key, private_target_key, streamer_state_key,
};
use crate::model::{NotifyTarget, StreamerState, TargetSubscriptions};

#[derive(Clone)]
pub struct SubscriptionRepo {
    store: Arc<dyn Store>,
}

impl SubscriptionRepo {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    pub async fn subscribe_group(&self, group_id: i64, uid: u64) -> Result<bool> {
        self.subscribe_target(&group_target_key(group_id), uid)
            .await
    }

    pub async fn subscribe_private(&self, user_id: i64, uid: u64) -> Result<bool> {
        self.subscribe_target(&private_target_key(user_id), uid)
            .await
    }

    pub async fn unsubscribe_group(&self, group_id: i64, uid: u64) -> Result<bool> {
        self.unsubscribe_target(&group_target_key(group_id), uid)
            .await
    }

    pub async fn unsubscribe_private(&self, user_id: i64, uid: u64) -> Result<bool> {
        self.unsubscribe_target(&private_target_key(user_id), uid)
            .await
    }

    pub async fn list_group(&self, group_id: i64) -> Result<Vec<u64>> {
        self.list_target(&group_target_key(group_id)).await
    }

    pub async fn list_private(&self, user_id: i64) -> Result<Vec<u64>> {
        self.list_target(&private_target_key(user_id)).await
    }

    pub async fn get_streamer_state(&self, uid: u64) -> Result<Option<StreamerState>> {
        self.store.get_json(&streamer_state_key(uid)).await
    }

    pub async fn set_streamer_state(&self, state: &StreamerState) -> Result<()> {
        self.store
            .set_json(&streamer_state_key(state.uid), state)
            .await
    }

    pub async fn subscriptions_by_uid(&self) -> Result<BTreeMap<u64, Vec<NotifyTarget>>> {
        let mut mapping = BTreeMap::<u64, Vec<NotifyTarget>>::new();

        for key in self.store.list_prefix(TARGET_GROUP_PREFIX).await? {
            if let Some(group_id) = parse_group_target_key(&key) {
                for uid in self.list_target(&key).await? {
                    mapping
                        .entry(uid)
                        .or_default()
                        .push(NotifyTarget::Group(group_id));
                }
            }
        }

        for key in self.store.list_prefix(TARGET_PRIVATE_PREFIX).await? {
            if let Some(user_id) = parse_private_target_key(&key) {
                for uid in self.list_target(&key).await? {
                    mapping
                        .entry(uid)
                        .or_default()
                        .push(NotifyTarget::Private(user_id));
                }
            }
        }

        Ok(mapping)
    }

    async fn subscribe_target(&self, key: &str, uid: u64) -> Result<bool> {
        let mut subs = self.load_target(key).await?;
        let inserted = subs.uids.insert(uid);
        self.store.set_json(key, &subs).await?;
        Ok(inserted)
    }

    async fn unsubscribe_target(&self, key: &str, uid: u64) -> Result<bool> {
        let mut subs = self.load_target(key).await?;
        let removed = subs.uids.remove(&uid);

        if subs.uids.is_empty() {
            let _ = self.store.delete(key).await?;
        } else {
            self.store.set_json(key, &subs).await?;
        }

        Ok(removed)
    }

    async fn list_target(&self, key: &str) -> Result<Vec<u64>> {
        Ok(self
            .load_target(key)
            .await?
            .uids
            .into_iter()
            .collect::<Vec<_>>())
    }

    async fn load_target(&self, key: &str) -> Result<TargetSubscriptions> {
        Ok(self.store.get_json(key).await?.unwrap_or_default())
    }
}
