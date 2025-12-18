use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::mpsc;

use crate::{adapter::onebot::v11::ctx::Ctx, core::extract::FromEvent};

type SessionKey = (i64, Option<i64>); // user_id, group_id
pub type Filter = Arc<dyn Fn(&Ctx) -> bool + Send + Sync>;

/// Manages conversation sessions and message interception
pub struct SessionManager {
    waiters: DashMap<SessionKey, (mpsc::Sender<Ctx>, Option<Filter>)>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            waiters: DashMap::new(),
        }
    }

    /// Register a waiter for a specific user/group with an optional filter
    pub fn register(&self, key: SessionKey, tx: mpsc::Sender<Ctx>, filter: Option<Filter>) {
        self.waiters.insert(key, (tx, filter));
    }

    /// Remove a waiter
    pub fn unregister(&self, key: &SessionKey) {
        self.waiters.remove(key);
    }

    /// Get the waiter channel and filter if it exists
    pub fn get_waiter(&self, key: &SessionKey) -> Option<(mpsc::Sender<Ctx>, Option<Filter>)> {
        self.waiters.get(key).map(|v| v.value().clone())
    }
}

/// A session handle for the current conversation
pub struct Session {
    key: SessionKey,
    manager: Arc<SessionManager>,
}

impl Session {
    pub fn new(user_id: i64, group_id: Option<i64>, manager: Arc<SessionManager>) -> Self {
        Self {
            key: (user_id, group_id),
            manager,
        }
    }

    /// Wait for the next message from this user/group
    ///
    /// This function intercepts the next message that would normally be dispatched to plugins
    /// and returns it here instead.
    pub async fn wait_next(&self) -> Option<Ctx> {
        self.wait_for(|_| true).await
    }

    /// Wait for the next message that matches the filter
    pub async fn wait_for(
        &self,
        filter: impl Fn(&Ctx) -> bool + Send + Sync + 'static,
    ) -> Option<Ctx> {
        let (tx, mut rx) = mpsc::channel(1);
        self.manager.register(self.key, tx, Some(Arc::new(filter)));

        let result = rx.recv().await;

        // Always unregister after receiving (one-shot interception)
        // If the user wants to continue waiting, they call wait_next again
        self.manager.unregister(&self.key);

        result
    }
}

#[async_trait]
impl FromEvent for Session {
    type Error = anyhow::Error;

    async fn from_event(ctx: &Ctx) -> Result<Self, Self::Error> {
        let manager = ctx.session_manager.clone();
        Ok(Session::new(ctx.user_id(), ctx.group_id(), manager))
    }
}
