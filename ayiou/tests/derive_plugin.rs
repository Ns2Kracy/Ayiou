use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use ayiou::Plugin;
use ayiou::core::adapter::MsgContext;
use ayiou::core::plugin::Plugin as PluginTrait;
use ayiou::core::plugin_host::PluginHost;
use ayiou::core::scheduler::{Scheduler, TokioScheduler};
use ayiou::core::storage::{MemoryStore, Store};

#[derive(Clone, Default)]
struct TestCtx {
    text: String,
}

impl MsgContext for TestCtx {
    fn text(&self) -> String {
        self.text.clone()
    }

    fn user_id(&self) -> String {
        "user".to_string()
    }

    fn group_id(&self) -> Option<String> {
        None
    }
}

#[derive(Plugin)]
#[plugin(
    name = "advanced-derived",
    context = "TestCtx",
    start = "bootstrap",
    handler = "dispatch"
)]
struct AdvancedDerivedPlugin {
    starts: Arc<AtomicUsize>,
    handles: Arc<AtomicUsize>,
}

impl AdvancedDerivedPlugin {
    async fn bootstrap(&self, _host: PluginHost<TestCtx>) -> Result<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn dispatch(&self, _ctx: &TestCtx) -> Result<bool> {
        self.handles.fetch_add(1, Ordering::SeqCst);
        Ok(false)
    }
}

#[tokio::test]
async fn derive_plugin_supports_custom_start_and_handler_methods() {
    let plugin = AdvancedDerivedPlugin {
        starts: Arc::new(AtomicUsize::new(0)),
        handles: Arc::new(AtomicUsize::new(0)),
    };
    let scheduler: Arc<dyn Scheduler> = Arc::new(TokioScheduler::new());
    let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
    let host = PluginHost::new(scheduler, store, None);

    PluginTrait::start(&plugin, host).await.unwrap();
    let blocked = PluginTrait::handle(&plugin, &TestCtx::default())
        .await
        .unwrap();

    assert_eq!(plugin.starts.load(Ordering::SeqCst), 1);
    assert_eq!(plugin.handles.load(Ordering::SeqCst), 1);
    assert!(!blocked);
    assert_eq!(PluginTrait::commands(&plugin), vec!["advanced-derived"]);
}
