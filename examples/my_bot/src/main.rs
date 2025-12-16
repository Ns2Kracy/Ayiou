use ayiou::AyiouBot;
use log::info;

mod commands;
mod plugin;

use plugin::DemoPlugin;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    info!("Starting Ayiou Full Feature Demo Bot...");

    let onebot_ws_url =
        std::env::var("ONEBOT_WS_URL").unwrap_or_else(|_| "ws://127.0.0.1:6700".to_string());

    let bot = AyiouBot::new().plugin::<DemoPlugin>();

    info!("Connecting to {}", onebot_ws_url);
    bot.run(onebot_ws_url).await;
}
