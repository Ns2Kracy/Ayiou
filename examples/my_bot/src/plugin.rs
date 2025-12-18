use ayiou::prelude::*;

// ============================================================================
// Function-based Commands using #[command] macro
// The macro generates struct + Args + Command + Plugin impl automatically
// ============================================================================

/// Echo command - repeats what you say
#[plugin(name = "echo", description = "Repeats what you say")]
pub async fn echo(ctx: Ctx, #[rest] content: String) -> anyhow::Result<()> {
    ctx.reply_text(format!("Echo: {}", content)).await?;
    Ok(())
}

/// Add command - adds two numbers
#[plugin(name = "add", description = "Adds two numbers")]
pub async fn add(ctx: Ctx, a: i32, b: i32) -> anyhow::Result<()> {
    ctx.reply_text(format!("{} + {} = {}", a, b, a + b)).await?;
    Ok(())
}

/// WhoAmI command - shows user info
#[plugin(name = "whoami", description = "Shows user info")]
pub async fn whoami(ctx: Ctx) -> anyhow::Result<()> {
    let user_id = ctx.user_id();
    let nickname = ctx.nickname();
    let mut msg = format!("You are {} ({})", nickname, user_id);
    if let Some(gid) = ctx.group_id() {
        msg.push_str(&format!("\nIn Group: {}", gid));
    } else {
        msg.push_str("\nIn Private Chat");
    }
    ctx.reply_text(msg).await?;
    Ok(())
}

/// Guess command - guessing game
#[plugin(name = "guess", description = "Guessing game")]
pub async fn guess(ctx: Ctx) -> anyhow::Result<()> {
    let secret_number = 42;
    ctx.reply_text(
        "I'm thinking of a number between 1 and 100. Try to guess it! (Type 'exit' to quit)",
    )
    .await?;

    let session = ayiou::core::session::Session::new(
        ctx.user_id(),
        ctx.group_id(),
        ctx.session_manager.clone(),
    );

    loop {
        let next_msg_result =
            tokio::time::timeout(std::time::Duration::from_secs(30), session.wait_next()).await;

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
