use std::sync::{Arc, Mutex};

use anyhow::Result;
use ayiou::core::adapter::MsgContext;
use ayiou::core::model::CommandInvocation;
use ayiou::core::plugin::{HandlerDecl, RuntimePlugin};
#[allow(unused_imports)]
use ayiou::{command, plugin};

#[derive(Clone, Default)]
struct TestCtx;

impl MsgContext for TestCtx {
    fn text(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed("")
    }

    fn user_id(&self) -> std::borrow::Cow<'_, str> {
        std::borrow::Cow::Borrowed("user")
    }

    fn group_id(&self) -> Option<std::borrow::Cow<'_, str>> {
        None
    }
}

struct ToolPlugin {
    seen: Arc<Mutex<Vec<String>>>,
}

struct SimplePlugin;

#[plugin(
    name = "tool",
    description = "tool command plugin",
    prefix = "/",
    context = "TestCtx",
    register = false
)]
impl ToolPlugin {
    #[command(name = "echo", alias = "say")]
    async fn echo(&self, _ctx: &TestCtx, content: String) -> Result<()> {
        self.seen.lock().unwrap().push(content);
        Ok(())
    }
}

#[plugin(name = "simple", prefix = "/", context = "TestCtx", register = false)]
impl SimplePlugin {
    async fn ping(&self, _ctx: &TestCtx) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn plugin_macro_registers_commands_and_dispatches_invocations() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let plugin = ToolPlugin { seen: seen.clone() };

    assert_eq!(RuntimePlugin::kind(&plugin), "tool");
    assert_eq!(
        RuntimePlugin::manifest(&plugin).description,
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

#[tokio::test]
async fn plugin_macro_treats_async_methods_as_commands() {
    let plugin = SimplePlugin;

    assert_eq!(RuntimePlugin::kind(&plugin), "simple");
    assert_eq!(
        RuntimePlugin::declared_handlers(&plugin),
        vec![HandlerDecl::message_commands(["ping"], ["/"])]
    );

    let outcome = RuntimePlugin::handle_with_invocation(
        &plugin,
        &TestCtx,
        Some(CommandInvocation::new("ping", "", Some("/"))),
    )
    .await
    .unwrap();

    assert!(outcome.block);
}
