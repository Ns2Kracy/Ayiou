use crate::commands;

use ayiou::prelude::*;

// ============================================================================
// Plugin Definition
// ============================================================================

#[derive(Plugin)]
#[plugin(
    name = "DemoBot",
    version = "1.0.0",
    description = "A full-featured demo plugin"
)]
pub enum DemoPlugin {
    #[plugin(
        rename = "echo",
        description = "Repeats what you say",
        handler = "crate::commands::echo::handle"
    )]
    Echo(commands::echo::EchoArgs),

    #[plugin(
        rename = "add",
        description = "Adds two numbers",
        handler = "crate::commands::add::handle"
    )]
    Add(commands::add::AddArgs),

    #[plugin(
        rename = "email",
        description = "Validates an email address",
        handler = "crate::commands::email::handle"
    )]
    Email(commands::email::EmailArgs),

    #[plugin(
        rename = "timer",
        description = "Checks cron schedule",
        handler = "crate::commands::timer::handle"
    )]
    Timer(commands::timer::TimerArgs),

    #[plugin(
        rename = "whoami",
        description = "Shows user info",
        handler = "crate::commands::whoami::handle"
    )]
    WhoAmI,

    #[plugin(
        rename = "guess",
        description = "Guessing game",
        handler = "crate::commands::guess::handle"
    )]
    Guess,

    #[plugin(
        rename = "help",
        description = "Show help",
        handler = "crate::commands::help::handle"
    )]
    Help,
}
