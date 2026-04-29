use std::sync::{Arc, Mutex};

use anyhow::Result;
use ayiou::core::adapter::MsgContext;
use ayiou::core::model::CommandInvocation;
use ayiou::core::plugin_system::{HandlerDecl, RuntimePlugin};
#[allow(unused_imports)]
use ayiou::{command, plugin};

#[derive(Clone, Default)]
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

struct ToolPlugin {
    seen: Arc<Mutex<Vec<String>>>,
}

#[plugin(
    name = "tool",
    description = "tool command plugin",
    prefix = "/",
    context = "TestCtx"
)]
impl ToolPlugin {
    #[command(name = "echo", alias = "say")]
    async fn echo(&self, _ctx: &TestCtx, content: String) -> Result<()> {
        self.seen.lock().unwrap().push(content);
        Ok(())
    }
}

#[tokio::test]
async fn plugin_macro_registers_commands_and_dispatches_invocations() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let plugin = ToolPlugin { seen: seen.clone() };

    assert_eq!(RuntimePlugin::kind(&plugin), "tool");
    assert_eq!(
        RuntimePlugin::meta(&plugin).description,
        "tool command plugin"
    );
    assert_eq!(
        RuntimePlugin::declared_handlers(&plugin),
        vec![HandlerDecl::message_commands(["echo", "say"], ["/"])]
    );

    let outcome = RuntimePlugin::handle_with_invocation(
        &plugin,
        &TestCtx,
        Some(CommandInvocation::new("say", "hello world", Some("/"))),
    )
    .await
    .unwrap();

    assert!(outcome.block);
    assert_eq!(*seen.lock().unwrap(), vec!["hello world".to_string()]);
}
