use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use ayiou::Bot;
use ayiou::core::adapter::{Adapter, MsgContext};
use ayiou::core::plugin::PluginMetadata;
use ayiou::core::plugin_system::{
    HandleOutcome, HandlerDecl, RuntimePlugin, RuntimePluginServices,
};
use tokio::sync::mpsc;

#[derive(Clone)]
struct TestCtx;

impl MsgContext for TestCtx {
    fn text(&self) -> String {
        String::new()
    }

    fn user_id(&self) -> String {
        "user".to_string()
    }

    fn group_id(&self) -> Option<String> {
        None
    }
}

struct ClosedAdapter;

#[async_trait]
impl Adapter for ClosedAdapter {
    type Ctx = TestCtx;

    async fn start(self) -> mpsc::Receiver<Self::Ctx> {
        let (_tx, rx) = mpsc::channel(1);
        rx
    }
}

struct StartPlugin {
    starts: Arc<AtomicUsize>,
}

#[async_trait]
impl RuntimePlugin<TestCtx> for StartPlugin {
    fn kind(&self) -> &str {
        "start-plugin"
    }

    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("start-plugin")
    }

    fn declared_handlers(&self) -> Vec<HandlerDecl> {
        vec![HandlerDecl::wildcard_message()]
    }

    async fn start(&mut self, _services: RuntimePluginServices<TestCtx>) -> Result<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn handle(&self, _ctx: &TestCtx) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

#[tokio::test]
async fn bot_invokes_plugin_start_once_before_exit() {
    let starts = Arc::new(AtomicUsize::new(0));
    let bot = Bot::new(ClosedAdapter).with_plugin(StartPlugin {
        starts: starts.clone(),
    });

    tokio::time::timeout(Duration::from_millis(200), bot.run())
        .await
        .expect("bot should exit when adapter channel closes");

    assert_eq!(starts.load(Ordering::SeqCst), 1);
}
