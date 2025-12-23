use async_trait::async_trait;
use tokio::sync::mpsc;

/// Trait for Contexts that support message operations (extracting text, session info)
pub trait MsgContext: Send + Sync + Clone + 'static {
    fn text(&self) -> String;
    fn user_id(&self) -> String;
    fn group_id(&self) -> Option<String>;
}

/// Adapter trait
///
/// Adapters are responsible for:
/// 1. Connecting to the platform (using a Driver)
/// 2. receiving raw events and converting them to Abstract Events
/// 3. Sending messages back to the platform
#[async_trait]
pub trait Adapter: Send + Sync + 'static {
    /// The context type this adapter's plugins will use
    type Ctx: MsgContext;

    /// Start the adapter
    ///
    /// Returns a channel of events
    async fn start(self) -> mpsc::Receiver<Self::Ctx>;
}
