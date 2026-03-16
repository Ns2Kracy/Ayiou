# ayiou-plugin-bilibili-live

Ayiou 的 B 站直播订阅插件。

## 功能

- `/blive sub <uid>`：订阅主播
- `/blive unsub <uid>`：取消订阅
- `/blive list`：查看当前群或当前私聊下的订阅
- 主播从“未开播 -> 开播”时自动推送通知
- 如果主播有封面图，通知会附带封面图片
- 如果订阅时主播已经在直播，订阅回执会直接合并标题、链接和封面
- Bot 启动后的第一次轮询只同步当前状态，不会把“已经在播”的主播误报成新开播

## 依赖

- 需要使用支持主动消息的 `OneBot v11` 适配器
- 需要给 `Bot` 配置可持久化的 `Store`

## 环境变量

- `BLIVE_POLL_INTERVAL_SECS`：轮询间隔秒数，可选，默认 `30`

如果你使用 `examples/my_bot` 一类的 OneBot 示例程序，通常还会用到：

- `DATABASE_URL`
- `ONEBOT_WS_URL`
- `ONEBOT_ACCESS_TOKEN`

## 使用示例

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

## 通知行为

### 订阅时主播已经开播

会返回一条合并消息，而不是“订阅成功”和“开播通知”各发一条：

```text
已订阅 JDG-mmonk1(44235058)
当前正在直播
标题：和菠萝双排亚服
链接：https://live.bilibili.com/8948888
```

如果接口返回了封面地址，这条消息还会附带封面图。

### 正常开播通知

插件只会在主播状态从“未开播”变成“开播”时推送。重复轮询到“仍在直播”不会重复提醒。

### Bot 重启

Bot 启动后的第一轮轮询会先建立当前直播状态基线，不会把已经在直播的主播误报成“刚开播”。

## 说明

- 订阅是按“群 / 私聊目标”隔离存储的
- 列表里的状态依赖最近一次轮询或订阅时同步到的主播状态
- `uid` 必须是 B 站用户 UID，不是直播间号
