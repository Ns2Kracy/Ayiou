//! Ayiou Command Demo
//!
//! This example demonstrates how to create command plugins using derive macros.
//! It shows:
//! - Basic command definition with #[derive(Plugin)]
//! - Argument parsing with #[derive(Args)]
//! - Different argument types (rest, optional, regex)
//!
//! Run with: cargo run -p ayiou --example command_demo
//!
//! NOTE: This example won't actually connect to OneBot - it's a demonstration
//! of the command parsing and handling patterns.

use anyhow::Result;
use ayiou::prelude::*;
use tracing::{info, Level};

// =============================================================================
// Simple Commands (Unit variants)
// =============================================================================

/// Define a simple plugin with basic commands
#[derive(Plugin)]
#[plugin(
    prefix = "/",
    name = "basic-commands",
    description = "Basic utility commands",
    version = "1.0.0"
)]
pub enum BasicCommands {
    /// Show help message
    #[plugin(description = "Show help message", alias = "?")]
    Help,

    /// Ping the bot
    #[plugin(description = "Ping pong test")]
    Ping,

    /// Get current time
    #[plugin(description = "Show server time", alias = "now")]
    Time,
}

// Implement handlers for unit variants
impl BasicCommands {
    pub async fn handle_help(ctx: &Ctx) -> Result<()> {
        ctx.reply_text(Self::help_text()).await
    }

    pub async fn handle_ping(ctx: &Ctx) -> Result<()> {
        ctx.reply_text("Pong!").await
    }

    pub async fn handle_time(ctx: &Ctx) -> Result<()> {
        let now = chrono::Local::now();
        ctx.reply_text(format!("Server time: {}", now.format("%Y-%m-%d %H:%M:%S")))
            .await
    }
}

// =============================================================================
// Commands with Arguments
// =============================================================================

/// Echo command arguments
#[derive(Args, Default)]
#[arg(usage = "/echo <message>")]
pub struct EchoArgs {
    /// The text to echo back (consumes all remaining input)
    #[arg(rest)]
    pub text: String,
}

impl EchoArgs {
    pub async fn handle(&self, ctx: &Ctx) -> Result<()> {
        if self.text.is_empty() {
            ctx.reply_text("Usage: /echo <message>").await
        } else {
            ctx.reply_text(format!("Echo: {}", self.text)).await
        }
    }
}

/// Say command with optional target
#[derive(Args, Default)]
#[arg(usage = "/say [target] <message>")]
pub struct SayArgs {
    pub target: String,
    #[arg(rest)]
    pub message: String,
}

impl SayArgs {
    pub async fn handle(&self, ctx: &Ctx) -> Result<()> {
        if self.message.is_empty() {
            // Only one arg provided, treat it as message
            ctx.reply_text(&self.target).await
        } else {
            ctx.reply_text(format!("To {}: {}", self.target, self.message))
                .await
        }
    }
}

/// Validated input example
#[derive(Args, Default)]
#[arg(usage = "/code <4-digit-code>")]
pub struct CodeArgs {
    #[arg(regex = r"^\d{4}$", error = "Code must be exactly 4 digits")]
    pub code: String,
}

impl CodeArgs {
    pub async fn handle(&self, ctx: &Ctx) -> Result<()> {
        ctx.reply_text(format!("Code accepted: {}", self.code))
            .await
    }
}

/// Commands with arguments
#[derive(Plugin)]
#[plugin(
    prefix = "/",
    name = "arg-commands",
    description = "Commands with arguments"
)]
pub enum ArgCommands {
    /// Echo back the message
    #[plugin(description = "Echo back text", alias = "e")]
    Echo(EchoArgs),

    /// Say something
    #[plugin(description = "Say a message")]
    Say(SayArgs),

    /// Submit a code
    #[plugin(description = "Submit 4-digit code")]
    Code(CodeArgs),
}

// =============================================================================
// Demo: Test parsing without actual OneBot connection
// =============================================================================

fn test_parsing() {
    println!("\n=== BasicCommands Parsing Test ===\n");

    let test_cases = ["/help", "/ping", "/time", "/now", "/?", "/unknown"];

    for text in test_cases {
        let matches = BasicCommands::matches_cmd(text);
        print!("  '{}' -> matches={}", text, matches);
        if matches {
            match BasicCommands::try_parse(text) {
                Ok(cmd) => println!(" (parsed: {:?})", std::any::type_name_of_val(&cmd)),
                Err(e) => println!(" (error: {})", e),
            }
        } else {
            println!();
        }
    }

    println!("\n=== ArgCommands Parsing Test ===\n");

    let arg_cases = [
        "/echo hello world",
        "/echo",
        "/e hello",
        "/say Alice Hi there!",
        "/say hello",
        "/code 1234",
        "/code abc",
        "/code 12345",
    ];

    for text in arg_cases {
        let matches = ArgCommands::matches_cmd(text);
        print!("  '{}' -> matches={}", text, matches);
        if matches {
            match ArgCommands::try_parse(text) {
                Ok(_) => println!(" (OK)"),
                Err(e) => println!(" (error: {})", e),
            }
        } else {
            println!();
        }
    }

    println!("\n=== Help Text ===\n");
    println!("BasicCommands:\n{}\n", BasicCommands::help_text());
    println!("ArgCommands:\n{}\n", ArgCommands::help_text());
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting Command Demo...");

    // Run parsing tests
    test_parsing();

    // Demonstrate building an app with these plugins
    info!("\n=== Building App with Command Plugins ===");

    let mut builder = AppBuilder::new();

    // Register both command plugins
    builder.add_plugin(BasicCommands::default())?;
    builder.add_plugin(ArgCommands::default())?;

    // Build the app
    let app = builder.build().await?;

    info!("App built with {} plugins:", app.plugins().len());
    for plugin in app.plugins().iter() {
        let meta = plugin.meta();
        info!("  - {} v{}: {}", meta.name, meta.version, meta.description);
    }

    info!("\nDemo complete! In a real app, use AyiouBot::run() to connect to OneBot.");
    info!("See ayiou-runner for a complete example.");

    Ok(())
}
