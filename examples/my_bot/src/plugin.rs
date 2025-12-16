use ayiou::prelude::*;

// ============================================================================
// Plugin Definition - Macro handles parsing, you handle business logic!
// ============================================================================

#[derive(Plugin)]
#[plugin(
    name = "DemoBot",
    version = "1.0.0",
    description = "A full-featured demo plugin"
)]
pub enum DemoPlugin {
    #[plugin(rename = "echo", description = "Repeats what you say")]
    Echo {
        #[plugin(rest)]
        content: String,
    },

    #[plugin(rename = "add", description = "Adds two numbers")]
    Add { a: i32, b: i32 },

    #[plugin(rename = "email", description = "Validates an email address")]
    Email {
        #[plugin(regex = r"^[\w\-\.]+@([\w-]+\.)+[\w-]{2,4}$")]
        email: RegexValidated,
    },

    #[plugin(rename = "timer", description = "Checks cron schedule")]
    Timer {
        #[plugin(cron)]
        schedule: Box<CronSchedule>,
    },

    #[plugin(rename = "whoami", description = "Shows user info")]
    WhoAmI,

    #[plugin(rename = "guess", description = "Guessing game")]
    Guess,

    #[plugin(rename = "help", description = "Show help")]
    Help,
}

// ============================================================================
// Business Logic - Implement execute() yourself, full control!
// ============================================================================

impl DemoPlugin {
    pub async fn execute(self, ctx: Ctx) -> anyhow::Result<()> {
        match self {
            Self::Echo { content } => {
                ctx.reply_text(format!("Echo: {}", content)).await?;
            }
            Self::Add { a, b } => {
                ctx.reply_text(format!("{} + {} = {}", a, b, a + b)).await?;
            }
            Self::Email { email } => {
                ctx.reply_text(format!("Valid email: {}", email)).await?;
            }
            Self::Timer { schedule } => {
                let next = schedule.upcoming().next().unwrap();
                ctx.reply_text(format!("Next trigger: {}", next)).await?;
            }
            Self::WhoAmI => {
                let user_id = ctx.user_id();
                let nickname = ctx.nickname();
                let mut msg = format!("You are {} ({})", nickname, user_id);
                if let Some(gid) = ctx.group_id() {
                    msg.push_str(&format!("\nIn Group: {}", gid));
                } else {
                    msg.push_str("\nIn Private Chat");
                }
                ctx.reply_text(msg).await?;
            }
            Self::Guess => {
                crate::commands::guess::handle(ctx).await?;
            }
            Self::Help => {
                ctx.reply_text(Self::help_text()).await?;
            }
        }
        Ok(())
    }
}
