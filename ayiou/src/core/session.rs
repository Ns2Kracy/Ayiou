use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{Mutex, OwnedMutexGuard};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConversationScope {
    User,
    Channel,
    Group,
    PluginInstance,
    Custom(String),
}

impl ConversationScope {
    pub fn custom(key: impl Into<String>) -> Self {
        Self::Custom(key.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub bot_id: String,
    pub platform: String,
    pub plugin: String,
    pub user_id: String,
    pub channel_id: Option<String>,
}

impl SessionKey {
    pub fn new(
        bot_id: impl Into<String>,
        platform: impl Into<String>,
        plugin: impl Into<String>,
        user_id: impl Into<String>,
        channel_id: Option<impl Into<String>>,
    ) -> Self {
        Self {
            bot_id: bot_id.into(),
            platform: platform.into(),
            plugin: plugin.into(),
            user_id: user_id.into(),
            channel_id: channel_id.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCursor {
    key: SessionKey,
    revision: u64,
}

impl SessionCursor {
    pub fn new(key: SessionKey, revision: u64) -> Self {
        Self { key, revision }
    }

    pub fn key(&self) -> &SessionKey {
        &self.key
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationStatus {
    Waiting,
    Paused { reason: String },
    Rejected { prompt: String },
    Finished,
}

#[derive(Debug, Clone)]
pub struct Conversation {
    key: SessionKey,
    record: Option<SessionRecord>,
}

impl Conversation {
    pub async fn resume(store: &dyn SessionStore, key: SessionKey) -> Result<Self> {
        let record = store.load(&key).await?;
        Ok(Self { key, record })
    }

    pub fn cursor(&self) -> Option<SessionCursor> {
        self.record
            .as_ref()
            .map(|record| SessionCursor::new(self.key.clone(), record.revision))
    }

    pub fn state(&self) -> Option<&Value> {
        self.record.as_ref().map(|record| &record.state)
    }

    pub async fn wait_next(
        &mut self,
        store: &dyn SessionStore,
        state: Value,
        ttl: Option<Duration>,
    ) -> Result<SessionCursor> {
        self.save_status(store, ConversationStatus::Waiting, state, ttl)
            .await
    }

    pub async fn pause(
        &mut self,
        store: &dyn SessionStore,
        reason: impl Into<String>,
        ttl: Option<Duration>,
    ) -> Result<SessionCursor> {
        self.save_status(
            store,
            ConversationStatus::Paused {
                reason: reason.into(),
            },
            serde_json::json!({}),
            ttl,
        )
        .await
    }

    pub async fn reject(
        &mut self,
        store: &dyn SessionStore,
        prompt: impl Into<String>,
        ttl: Option<Duration>,
    ) -> Result<SessionCursor> {
        self.save_status(
            store,
            ConversationStatus::Rejected {
                prompt: prompt.into(),
            },
            serde_json::json!({}),
            ttl,
        )
        .await
    }

    pub async fn finish(&mut self, store: &dyn SessionStore) -> Result<bool> {
        let deleted = store.delete(&self.key).await?;
        self.record = None;
        Ok(deleted)
    }

    async fn save_status(
        &mut self,
        store: &dyn SessionStore,
        status: ConversationStatus,
        state: Value,
        ttl: Option<Duration>,
    ) -> Result<SessionCursor> {
        let state = serde_json::json!({
            "status": conversation_status_value(&status),
            "state": state,
        });
        let record = store.save(self.key.clone(), state, ttl).await?;
        let cursor = SessionCursor::new(self.key.clone(), record.revision);
        self.record = Some(record);
        Ok(cursor)
    }
}

fn conversation_status_value(status: &ConversationStatus) -> Value {
    match status {
        ConversationStatus::Waiting => serde_json::json!({"kind": "waiting"}),
        ConversationStatus::Paused { reason } => {
            serde_json::json!({"kind": "paused", "reason": reason})
        }
        ConversationStatus::Rejected { prompt } => {
            serde_json::json!({"kind": "rejected", "prompt": prompt})
        }
        ConversationStatus::Finished => serde_json::json!({"kind": "finished"}),
    }
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub state: Value,
    pub revision: u64,
    pub expires_at: Option<Instant>,
}

impl SessionRecord {
    pub fn is_expired(&self, now: Instant) -> bool {
        self.expires_at.is_some_and(|at| at <= now)
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session not found")]
    NotFound,
    #[error("session expired")]
    Expired,
    #[error("session revision conflict: expected={expected}, actual={actual}")]
    RevisionConflict { expected: u64, actual: u64 },
}

#[async_trait]
pub trait SessionStore: Send + Sync + 'static {
    async fn load(&self, key: &SessionKey) -> Result<Option<SessionRecord>>;
    async fn save(
        &self,
        key: SessionKey,
        state: Value,
        ttl: Option<Duration>,
    ) -> Result<SessionRecord>;
    async fn delete(&self, key: &SessionKey) -> Result<bool>;
    async fn update_if_revision(
        &self,
        key: &SessionKey,
        expected_revision: u64,
        state: Value,
        ttl: Option<Duration>,
    ) -> std::result::Result<SessionRecord, SessionError>;
    async fn cleanup_expired(&self) -> Result<usize>;
    async fn acquire_lock(&self, key: &SessionKey) -> Result<OwnedMutexGuard<()>>;
}

#[derive(Default)]
pub struct MemorySessionStore {
    sessions: DashMap<SessionKey, SessionRecord>,
    locks: DashMap<SessionKey, Arc<Mutex<()>>>,
}

impl MemorySessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn expiry(ttl: Option<Duration>) -> Option<Instant> {
        ttl.map(|ttl| Instant::now() + ttl)
    }

    fn lock_for(&self, key: &SessionKey) -> Arc<Mutex<()>> {
        self.locks
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn load(&self, key: &SessionKey) -> Result<Option<SessionRecord>> {
        let now = Instant::now();
        if let Some(record) = self.sessions.get(key) {
            if record.is_expired(now) {
                drop(record);
                self.sessions.remove(key);
                return Ok(None);
            }
            return Ok(Some(record.clone()));
        }
        Ok(None)
    }

    async fn save(
        &self,
        key: SessionKey,
        state: Value,
        ttl: Option<Duration>,
    ) -> Result<SessionRecord> {
        let revision = self.sessions.get(&key).map_or(1, |r| r.revision + 1);
        let record = SessionRecord {
            state,
            revision,
            expires_at: Self::expiry(ttl),
        };
        self.sessions.insert(key, record.clone());
        Ok(record)
    }

    async fn delete(&self, key: &SessionKey) -> Result<bool> {
        Ok(self.sessions.remove(key).is_some())
    }

    async fn update_if_revision(
        &self,
        key: &SessionKey,
        expected_revision: u64,
        state: Value,
        ttl: Option<Duration>,
    ) -> std::result::Result<SessionRecord, SessionError> {
        let now = Instant::now();
        let Some(current) = self.sessions.get(key) else {
            return Err(SessionError::NotFound);
        };

        if current.is_expired(now) {
            drop(current);
            self.sessions.remove(key);
            return Err(SessionError::Expired);
        }

        if current.revision != expected_revision {
            return Err(SessionError::RevisionConflict {
                expected: expected_revision,
                actual: current.revision,
            });
        }

        let next = SessionRecord {
            state,
            revision: current.revision + 1,
            expires_at: Self::expiry(ttl),
        };
        drop(current);
        self.sessions.insert(key.clone(), next.clone());
        Ok(next)
    }

    async fn cleanup_expired(&self) -> Result<usize> {
        let now = Instant::now();
        let mut expired = Vec::new();

        for entry in &self.sessions {
            if entry.value().is_expired(now) {
                expired.push(entry.key().clone());
            }
        }

        let n = expired.len();
        for key in expired {
            self.sessions.remove(&key);
        }
        Ok(n)
    }

    async fn acquire_lock(&self, key: &SessionKey) -> Result<OwnedMutexGuard<()>> {
        let lock = self.lock_for(key);
        Ok(lock.lock_owned().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn session_store_basic_flow() {
        let store = MemorySessionStore::new();
        let key = SessionKey::new("bot-a", "onebot", "wizard", "u1", Some("g1"));

        let saved = store
            .save(key.clone(), serde_json::json!({"step": 1}), None)
            .await
            .unwrap();
        assert_eq!(saved.revision, 1);

        let loaded = store.load(&key).await.unwrap().unwrap();
        assert_eq!(loaded.revision, 1);

        let updated = store
            .update_if_revision(&key, 1, serde_json::json!({"step": 2}), None)
            .await
            .unwrap();
        assert_eq!(updated.revision, 2);
    }

    #[tokio::test]
    async fn session_store_revision_conflict() {
        let store = MemorySessionStore::new();
        let key = SessionKey::new("bot-a", "onebot", "wizard", "u1", Some("g1"));

        store
            .save(key.clone(), serde_json::json!({"step": 1}), None)
            .await
            .unwrap();

        let err = store
            .update_if_revision(&key, 99, serde_json::json!({"step": 2}), None)
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            SessionError::RevisionConflict {
                expected: 99,
                actual: 1
            }
        ));
    }

    #[tokio::test]
    async fn conversation_cursor_can_pause_resume_and_finish() {
        let store = MemorySessionStore::new();
        let key = SessionKey::new("bot-a", "onebot", "wizard", "u1", Some("g1"));

        let mut conversation = Conversation::resume(&store, key.clone()).await.unwrap();
        assert!(conversation.cursor().is_none());

        let first = conversation
            .wait_next(&store, serde_json::json!({"step": 1}), None)
            .await
            .unwrap();
        assert_eq!(first.key(), &key);
        assert_eq!(first.revision(), 1);

        let resumed = Conversation::resume(&store, key.clone()).await.unwrap();
        assert_eq!(resumed.cursor().unwrap().revision(), 1);
        assert_eq!(resumed.state().unwrap()["status"]["kind"], "waiting");

        let mut resumed = resumed;
        let paused = resumed
            .pause(&store, "external review", None)
            .await
            .unwrap();
        assert_eq!(paused.revision(), 2);

        assert!(resumed.finish(&store).await.unwrap());
        assert!(store.load(&key).await.unwrap().is_none());
    }
}
