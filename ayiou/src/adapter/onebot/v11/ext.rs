use async_trait::async_trait;

use crate::{Bot, adapter::onebot::v11::adapter::OneBotV11Adapter};

#[async_trait]
pub trait OneBotV11BotExt {
    /// Apply OneBot-friendly defaults (currently command prefixes: / ! .)
    fn with_onebot_defaults(self) -> Self;

    /// Run bot with OneBot v11 websocket url.
    async fn run_onebot_ws(self, url: impl Into<String> + Send);
}

#[async_trait]
impl OneBotV11BotExt for Bot<OneBotV11Adapter> {
    fn with_onebot_defaults(self) -> Self {
        self.command_prefixes(["/", "!", "."])
    }

    async fn run_onebot_ws(self, url: impl Into<String> + Send) {
        self.run(OneBotV11Adapter::new(url)).await;
    }
}
