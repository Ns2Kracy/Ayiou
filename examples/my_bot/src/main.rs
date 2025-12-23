use ayiou::AyiouBot;
use log::info;

mod plugin;

// The #[plugin] macro generates:
// - A struct (e.g., Echo) for type-based registration
// - A factory function (e.g., echo()) for value-based registration
use plugin::{add, echo, guess, whoami};
// Or use the struct names: Echo, Add, Whoami, Guess

#[tokio::main]
async fn main() {
    pretty_env_logger::try_init().ok();
    info!("Starting Ayiou Full Feature Demo Bot...");

    let onebot_ws_url =
        std::env::var("ONEBOT_WS_URL").unwrap_or_else(|_| "ws://10.126.126.1:3001".to_string());

    // Register plugins using factory functions - more intuitive!
    let bot = AyiouBot::new()
        .register_plugin(echo)
        .register_plugin(add)
        .register_plugin(whoami)
        .register_plugin(guess);

    info!("Connecting to {}", onebot_ws_url);
    bot.run(onebot_ws_url).await;
}
