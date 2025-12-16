use ayiou::prelude::*;
use anyhow::Result;

pub async fn handle(ctx: Ctx) -> Result<()> {
    let secret_number = 42;
    ctx.reply_text("I'm thinking of a number between 1 and 100. Try to guess it! (Type 'exit' to quit)").await?;

    let session = ayiou::core::session::Session::new(
        ctx.user_id(),
        ctx.group_id(),
        ctx.session_manager.clone()
    );

    loop {
        let next_msg_result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            session.wait_next()
        ).await;

        let next_msg = match next_msg_result {
            Ok(Some(msg)) => msg,
            Ok(None) => break, // Channel closed
            Err(_) => {
                ctx.reply_text("Time's up! Game over.").await?;
                break;
            }
        };

        let text = next_msg.text();
        if text.trim().eq_ignore_ascii_case("exit") {
            next_msg.reply_text("Game cancelled.").await?;
            break;
        }

        if let Ok(guess) = text.trim().parse::<i32>() {
            if guess == secret_number {
                next_msg.reply_text("🎉 You guessed it! You win!").await?;
                break;
            } else if guess < secret_number {
                next_msg.reply_text("Too low! Try again.").await?;
            } else {
                next_msg.reply_text("Too high! Try again.").await?;
            }
        } else {
            next_msg.reply_text("Please enter a valid number.").await?;
        }
    }
    Ok(())
}
