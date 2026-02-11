use async_trait::async_trait;

use crate::{Bot, adapter::console::adapter::ConsoleAdapter};

#[async_trait]
pub trait ConsoleBotExt {
    fn with_console_defaults(self) -> Self;
    async fn run_console(self);
}

#[async_trait]
impl ConsoleBotExt for Bot<ConsoleAdapter> {
    fn with_console_defaults(self) -> Self {
        self.command_prefixes(["/", "!", "."])
    }

    async fn run_console(self) {
        self.run(ConsoleAdapter::new()).await;
    }
}
