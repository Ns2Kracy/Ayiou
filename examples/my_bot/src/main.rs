use ayiou::AyiouBot;
use log::info;

mod plugin;

use plugin::{AddPlugin, EchoPlugin, GuessPlugin, UrlDetectorPlugin, WhoamiPlugin};

#[tokio::main]
async fn main() {
    pretty_env_logger::try_init().ok();
    info!("Starting Ayiou Full Feature Demo Bot...");

    let onebot_ws_url =
        std::env::var("ONEBOT_WS_URL").unwrap_or_else(|_| "ws://10.126.126.1:3001".to_string());

    // Register plugins using derive macro structs
    let bot = AyiouBot::new()
        .register_plugin(EchoPlugin)
        .register_plugin(AddPlugin)
        .register_plugin(WhoamiPlugin)
        .register_plugin(GuessPlugin)
        .register_plugin(UrlDetectorPlugin);

    info!("Connecting to {}", onebot_ws_url);
    bot.run(onebot_ws_url).await;
}
