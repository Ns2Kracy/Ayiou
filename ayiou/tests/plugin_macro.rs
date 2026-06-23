use std::sync::{Arc, Mutex};

use anyhow::Result;
use ayiou::Context;
use ayiou::core::model::{BotId, CommandInvocation, EventEnvelope, PlatformId};
use ayiou::core::plugin::{CommandMeta, HandlerDecl, Permission, RuntimePlugin};
#[allow(unused_imports)]
use ayiou::{command, plugin};

struct ToolPlugin {
    seen: Arc<Mutex<Vec<String>>>,
}

struct SimplePlugin;

#[plugin(
    name = "tool",
    description = "tool command plugin",
    prefix = "/",
    register = false
)]
impl ToolPlugin {
    #[command(
        name = "echo",
        alias = "say",
        aliases = ["repeat"],
        summary = "Echo text",
        usage = "/echo <text>",
        examples = ["/echo hello", "/say hello"],
        priority = 10,
        block = false,
        permissions = ["chat.echo"]
    )]
    async fn echo(&self, _ctx: &Context, content: String) -> Result<()> {
        self.seen.lock().unwrap().push(content);
        Ok(())
    }
}

#[plugin(name = "simple", prefix = "/", register = false)]
impl SimplePlugin {
    async fn ping(&self, _ctx: &Context) -> Result<()> {
        Ok(())
    }
}

fn test_context() -> Context {
    let platform = PlatformId::new("test");
    Context::new(
        EventEnvelope::new(BotId::new("test-bot"), platform),
        None,
        (),
    )
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
        vec![
            HandlerDecl::message_commands(["echo", "say", "repeat"], ["/"])
                .command_meta([CommandMeta::new("echo")
                    .aliases(["say", "repeat"])
                    .summary("Echo text")
                    .usage("/echo <text>")
                    .examples(["/echo hello", "/say hello"])])
                .require_permissions([Permission::custom("chat.echo")])
                .priority(10)
                .block(false)
        ]
    );

    let ctx = test_context();
    let outcome = RuntimePlugin::handle_with_invocation(
        &plugin,
        &ctx,
        Some(CommandInvocation::new("say", "hello world", Some("/"))),
    )
    .await
    .unwrap();

    assert!(!outcome.block);
    assert_eq!(*seen.lock().unwrap(), vec!["hello world".to_string()]);
}

#[tokio::test]
async fn plugin_macro_treats_async_methods_as_commands() {
    let plugin = SimplePlugin;

    assert_eq!(RuntimePlugin::kind(&plugin), "simple");
    assert_eq!(
        RuntimePlugin::declared_handlers(&plugin),
        vec![
            HandlerDecl::message_commands(["ping"], ["/"])
                .command_meta([ayiou::core::plugin::CommandMeta::new("ping")])
                .block(true)
        ]
    );

    let ctx = test_context();
    let outcome = RuntimePlugin::handle_with_invocation(
        &plugin,
        &ctx,
        Some(CommandInvocation::new("ping", "", Some("/"))),
    )
    .await
    .unwrap();

    assert!(outcome.block);
}
