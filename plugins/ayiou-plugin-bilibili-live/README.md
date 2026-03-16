# ayiou-plugin-bilibili-live

订阅 B 站主播开播提醒的 Ayiou 插件。

## 功能

- `/blive sub <uid>`：订阅主播
- `/blive unsub <uid>`：取消订阅
- `/blive list`：查看当前群或当前私聊下的订阅
- 轮询检测“未开播 -> 开播”边沿，只在开播瞬间提醒一次
- 订阅与直播状态持久化到数据库 KV

## 运行要求

- 当前版本面向 `OneBot v11`
- 需要给 `Bot` 注入数据库版 `Store`

## 配置

- `DATABASE_URL`：数据库连接串，例如 `sqlite://data/ayiou.sqlite?mode=rwc`
- `BLIVE_POLL_INTERVAL_SECS`：轮询间隔秒数，可选，默认 `30`
- `ONEBOT_WS_URL`：OneBot WebSocket 地址；如果需要鉴权，可直接写成 `ws://127.0.0.1:3001/?access_token=your-token`
- `ONEBOT_ACCESS_TOKEN`：可选；若 `ONEBOT_WS_URL` 里还没有 `access_token`，示例 `my_bot` 会自动把它追加到 URL 查询参数中

## 注册方式

```rust
use std::sync::Arc;

use ayiou::prelude::*;
use ayiou_plugin_bilibili_live::BilibiliLivePlugin;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let store = Arc::new(SeaOrmStore::connect(
        &std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/ayiou.sqlite?mode=rwc".to_string()),
    )
    .await?);

    let bot = Bot::<ayiou::adapter::onebot::v11::adapter::OneBotV11Adapter>::new()
        .with_store(store)
        .register_plugin(BilibiliLivePlugin::new())
        .with_onebot_defaults();

    bot.run_onebot_ws("ws://127.0.0.1:3001").await;
    Ok(())
}
```

## 说明

- 在群里执行订阅命令，提醒会发到该群
- 在私聊里执行订阅命令，提醒会私聊发给该用户
- 当前只支持数字 `uid`
- `my_bot` 的日志会自动脱敏 `access_token`
