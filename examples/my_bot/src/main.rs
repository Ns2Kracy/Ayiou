use ayiou::{
    Bot,
    adapter::onebot::v11::{adapter::OneBotV11Adapter, ext::OneBotV11BotExt},
};
use log::info;

mod plugin;

use plugin::{AddPlugin, EchoPlugin, GuessPlugin, ToolboxPlugin, UrlDetectorPlugin, WhoamiPlugin};

#[tokio::main]
async fn main() {
    unsafe {
        std::env::set_var("RUST_LOG", "DEBUG");
    }

    pretty_env_logger::try_init().ok();
    info!("Starting Ayiou Full Feature Demo Bot...");

    let onebot_ws_url =
        std::env::var("ONEBOT_WS_URL").unwrap_or_else(|_| "ws://192.168.31.180:3001".to_string());

    let bot = Bot::<OneBotV11Adapter>::new()
        .with_onebot_defaults()
        .register_plugin(EchoPlugin)
        .register_plugin(AddPlugin)
        .register_plugin(WhoamiPlugin)
        .register_plugin(GuessPlugin)
        .register_plugin(UrlDetectorPlugin)
        .register_plugin(ToolboxPlugin);

    info!("Connecting to {}", onebot_ws_url);
    bot.run_onebot_ws(onebot_ws_url).await;
}
