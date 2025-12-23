use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::mpsc;

use crate::{core::adapter::MsgContext, core::extract::FromEvent};

type SessionKey = (String, Option<String>); // user_id, group_id
pub type Filter<C> = Arc<dyn Fn(&C) -> bool + Send + Sync>;

/// Manages conversation sessions and message interception
pub struct SessionManager<C> {
    waiters: DashMap<SessionKey, (mpsc::Sender<C>, Option<Filter<C>>)>,
}

impl<C> Default for SessionManager<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> SessionManager<C> {
    pub fn new() -> Self {
        Self {
            waiters: DashMap::new(),
        }
    }

    /// Register a waiter for a specific user/group with an optional filter
    pub fn register(&self, key: SessionKey, tx: mpsc::Sender<C>, filter: Option<Filter<C>>) {
        self.waiters.insert(key, (tx, filter));
    }

    /// Remove a waiter
    pub fn unregister(&self, key: &SessionKey) {
        self.waiters.remove(key);
    }

    /// Get the waiter channel and filter if it exists
    pub fn get_waiter(&self, key: &SessionKey) -> Option<(mpsc::Sender<C>, Option<Filter<C>>)> {
        self.waiters.get(key).map(|v| v.value().clone())
    }
}

/// A session handle for the current conversation
pub struct Session<C> {
    key: SessionKey,
    manager: Arc<SessionManager<C>>,
}

impl<C: MsgContext> Session<C> {
    pub fn new(
        user_id: impl Into<String>,
        group_id: Option<impl Into<String>>,
        manager: Arc<SessionManager<C>>,
    ) -> Self {
        Self {
            key: (user_id.into(), group_id.map(|g| g.into())),
            manager,
        }
    }

    /// Wait for the next message from this user/group
    ///
    /// This function intercepts the next message that would normally be dispatched to plugins
    /// and returns it here instead.
    pub async fn wait_next(&self) -> Option<C> {
        self.wait_for(|_| true).await
    }

    /// Wait for the next message that matches the filter
    pub async fn wait_for(&self, filter: impl Fn(&C) -> bool + Send + Sync + 'static) -> Option<C> {
        let (tx, mut rx) = mpsc::channel(1);
        self.manager
            .register(self.key.clone(), tx, Some(Arc::new(filter)));

        let result = rx.recv().await;

        // Always unregister after receiving (one-shot interception)
        // If the user wants to continue waiting, they call wait_next again
        self.manager.unregister(&self.key);

        result
    }
}

#[async_trait]
impl<C: MsgContext + FromEvent<C>> FromEvent<C> for Session<C> {
    type Error = anyhow::Error;

    // We can't implement this properly without C having access to SessionManager.
    // In OneBot Ctx, `ctx.session_manager` is available.
    // We need `MsgContext` to also provide access to `session_manager`?
    // OR we can make `Session` extraction depend on a new trait `SessionProvider`.
    // For now let's assume `FromEvent` can handle it if we fix the FromEvent trait.
    // But `Session::new` needs `manager`.
    // The previous implementation used `ctx.session_manager`.
    // If generic C does not expose session_manager, we can't extract Session.

    // I will add `session_manager()` to `MsgContext` or a separate trait later.
    // For now I'll comment out this impl or leave it incomplete?
    // No, I need it.
    // Let's assume `C` has `session_manager()` method?
    // Generic `MsgContext` didn't have it in previous file edit.
    // I will add `session_manager` getter to `MsgContext`?
    // But `MsgContext` is defined in `core/adapter.rs` and implementation is in adapter crate.
    // It returns `Arc<SessionManager<C>>`.

    // Wait, let's look at `core/adapter.rs` again. I didn't add `session_manager` to `MsgContext`.
    // I should add it.

    async fn from_event(_ctx: &C) -> Result<Self, Self::Error> {
        // Placeholder
        Err(anyhow::anyhow!(
            "Session extraction not yet implemented fully generic"
        ))
    }
}
