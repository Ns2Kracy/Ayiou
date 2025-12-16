use ayiou::prelude::*;
use ayiou::core::plugin::Command;
use anyhow::Result;
use async_trait::async_trait;

/// Arguments for the /timer command
#[derive(Debug, Args)]
pub struct TimerArgs {
    #[arg(cron)]
    pub schedule: CronSchedule,
}

#[async_trait]
impl Command for TimerArgs {
    async fn run(self, ctx: Ctx) -> Result<()> {
        let sched = self.schedule;
        let next = sched.upcoming().next().unwrap();
        ctx.reply_text(format!("Next trigger time for '{}' is: {}", sched, next)).await?;
        Ok(())
    }
}
