use ayiou::{
    core::{command, group, private, regex, MatcherExt, MessageHandler},
    AyiouBot,
};

// 处理函数 - 独立定义，方便复用和测试
async fn ping(ctx: ayiou::core::Ctx) -> anyhow::Result<bool> {
    ctx.reply_text("pong").await?;
    Ok(false) // false = 继续, true = 阻止后续
}

async fn hello(ctx: ayiou::core::Ctx) -> anyhow::Result<bool> {
    ctx.reply_text(format!("Hello, {}!", ctx.nickname()))
        .await?;
    Ok(false)
}

async fn echo(ctx: ayiou::core::Ctx) -> anyhow::Result<bool> {
    if let Some(content) = ctx.text().strip_prefix('!') {
        ctx.reply_text(format!("Echo: {}", content)).await?;
    }
    Ok(false)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    AyiouBot::new()
        // 命令处理器
        .handler(MessageHandler::new("ping", ping).with_matcher(command("/ping")))
        // 正则处理器
        .handler(MessageHandler::new("hello", hello).with_matcher(regex(r"^(hi|hello|你好)")))
        // 通用处理器（无匹配器 = 匹配所有）
        .handler(MessageHandler::new("echo", echo))
        // 组合匹配器示例：仅私聊 + 命令
        .handler(
            MessageHandler::new("private_help", |ctx: ayiou::core::Ctx| async move {
                ctx.reply_text("这是私聊帮助").await?;
                Ok(false)
            })
            .with_matcher(command("/help").and(private())),
        )
        // 组合匹配器示例：群聊 或 特定用户
        .handler(
            MessageHandler::new("admin", |ctx: ayiou::core::Ctx| async move {
                ctx.reply_text("管理员命令").await?;
                Ok(true) // 阻止后续
            })
            .with_matcher(command("/admin").and(group().or(ayiou::core::user(12345)))),
        )
        .connect("ws://192.168.31.180:3001")
        .run()
        .await;
}
