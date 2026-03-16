use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use log::warn;

use ayiou::adapter::onebot::v11::ctx::Ctx;
use ayiou::core::plugin::{Plugin, PluginMetadata};
use ayiou::core::plugin_host::PluginHost;
use ayiou::core::scheduler::schedule_job;
use ayiou::core::storage::Store;

use crate::client::{BilibiliLiveClient, LiveClient, LiveStatus, ResolveStreamer};
use crate::command::CommandAction;
use crate::model::StreamerState;
use crate::poller::Poller;
use crate::repo::SubscriptionRepo;

pub struct BilibiliLivePlugin {
    client: Arc<dyn LiveClient>,
    poll_interval: Duration,
    started: AtomicBool,
    store: OnceLock<Arc<dyn Store>>,
}

impl Default for BilibiliLivePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl BilibiliLivePlugin {
    pub fn new() -> Self {
        Self::with_client(Arc::new(BilibiliLiveClient::new()))
    }

    pub fn with_client(client: Arc<dyn LiveClient>) -> Self {
        Self {
            client,
            poll_interval: poll_interval_from_env(),
            started: AtomicBool::new(false),
            store: OnceLock::new(),
        }
    }

    fn repo(&self) -> Result<SubscriptionRepo> {
        let store = self
            .store
            .get()
            .cloned()
            .ok_or_else(|| anyhow!("bilibili live plugin is not initialized"))?;
        Ok(SubscriptionRepo::new(store))
    }

    async fn handle_sub(&self, ctx: &Ctx, uid: u64) -> Result<()> {
        let repo = self.repo()?;
        let streamer = self.client.resolve_streamer(uid).await?;
        let inserted = match ctx.group_id() {
            Some(group_id) => repo.subscribe_group(group_id, uid).await?,
            None => repo.subscribe_private(ctx.user_id(), uid).await?,
        };

        let live_status = self.client.fetch_live_status(streamer.room_id).await.ok();
        if let Some(status) = live_status.as_ref() {
            repo.set_streamer_state(&streamer_state_snapshot(&streamer, status))
                .await?;
        }

        ctx.reply_text(build_sub_reply(inserted, &streamer, live_status.as_ref()))
            .await
    }

    async fn handle_unsub(&self, ctx: &Ctx, uid: u64) -> Result<()> {
        let repo = self.repo()?;
        let removed = match ctx.group_id() {
            Some(group_id) => repo.unsubscribe_group(group_id, uid).await?,
            None => repo.unsubscribe_private(ctx.user_id(), uid).await?,
        };

        if removed {
            ctx.reply_text(format!("已取消订阅 {}", uid)).await?;
        } else {
            ctx.reply_text(format!("当前目标未订阅 {}", uid)).await?;
        }

        Ok(())
    }

    async fn handle_list(&self, ctx: &Ctx) -> Result<()> {
        let repo = self.repo()?;
        let uids = match ctx.group_id() {
            Some(group_id) => repo.list_group(group_id).await?,
            None => repo.list_private(ctx.user_id()).await?,
        };

        if uids.is_empty() {
            ctx.reply_text("当前没有订阅任何主播").await?;
            return Ok(());
        }

        let mut lines = Vec::new();
        for uid in uids {
            if let Some(state) = repo.get_streamer_state(uid).await? {
                let name = state.uname.unwrap_or_else(|| uid.to_string());
                let status = if state.is_live {
                    "直播中"
                } else {
                    "未开播"
                };
                lines.push(format!("{}({}) - {}", name, uid, status));
            } else {
                lines.push(uid.to_string());
            }
        }

        ctx.reply_text(lines.join("\n")).await?;
        Ok(())
    }

    async fn reply_usage(&self, ctx: &Ctx, reason: impl AsRef<str>) -> Result<()> {
        let usage = "Usage: /blive sub <uid> | /blive unsub <uid> | /blive list";
        let reason = reason.as_ref();
        if reason.is_empty() {
            ctx.reply_text(usage).await
        } else {
            ctx.reply_text(format!("{}\n{}", reason, usage)).await
        }
    }
}

#[async_trait]
impl Plugin<Ctx> for BilibiliLivePlugin {
    fn meta(&self) -> PluginMetadata {
        PluginMetadata::new("blive").description("Bilibili live notification plugin")
    }

    fn commands(&self) -> Vec<String> {
        vec!["blive".to_string()]
    }

    async fn start(&self, host: PluginHost<Ctx>) -> Result<()> {
        let store = host.store();
        let _ = self.store.get_or_init(|| store.clone());

        if self.started.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        let sender = host.require_sender()?;
        let poller = Poller::new(SubscriptionRepo::new(store), self.client.clone(), sender);
        let task = schedule_job(move || {
            let poller = poller.clone();
            async move {
                if let Err(err) = poller.poll_once().await {
                    warn!("bilibili live poll failed: {}", err);
                }
            }
        });

        host.scheduler()
            .schedule_interval(self.poll_interval, task)?;

        Ok(())
    }

    async fn handle(&self, ctx: &Ctx) -> Result<bool> {
        let args = ctx.command_args().unwrap_or_default();
        match CommandAction::parse(&args) {
            Ok(CommandAction::Sub { uid }) => self.handle_sub(ctx, uid).await?,
            Ok(CommandAction::Unsub { uid }) => self.handle_unsub(ctx, uid).await?,
            Ok(CommandAction::List) => self.handle_list(ctx).await?,
            Err(err) => self.reply_usage(ctx, err.to_string()).await?,
        }

        Ok(true)
    }
}

fn poll_interval_from_env() -> Duration {
    std::env::var("BLIVE_POLL_INTERVAL_SECS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(30))
}

fn build_sub_reply(
    inserted: bool,
    streamer: &ResolveStreamer,
    live_status: Option<&LiveStatus>,
) -> String {
    let mut reply = if inserted {
        format!("已订阅 {}({})", streamer.uname, streamer.uid)
    } else {
        format!("已订阅 {}({})，无需重复添加", streamer.uname, streamer.uid)
    };

    if let Some(status) = live_status.filter(|status| status.is_live) {
        reply.push_str("\n当前正在直播");
        if !status.title.is_empty() {
            reply.push_str(&format!("\n标题：{}", status.title));
        }
        reply.push_str(&format!("\n链接：{}", status.live_url));
    }

    reply
}

fn streamer_state_snapshot(streamer: &ResolveStreamer, status: &LiveStatus) -> StreamerState {
    StreamerState {
        uid: streamer.uid,
        room_id: Some(streamer.room_id),
        uname: Some(streamer.uname.clone()),
        is_live: status.is_live,
        title: Some(status.title.clone()),
        live_url: Some(status.live_url.clone()),
        cover_url: status.cover_url.clone(),
        last_live_started_at: status.live_started_at.clone(),
        last_seen_at: Some(Utc::now().to_rfc3339()),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_sub_reply, streamer_state_snapshot};
    use crate::client::{LiveStatus, ResolveStreamer};

    #[test]
    fn sub_reply_merges_live_details() {
        let streamer = ResolveStreamer {
            uid: 44_235_058,
            uname: "JDG-mmonk1".to_string(),
            room_id: 8_948_888,
        };
        let status = LiveStatus {
            room_id: 8_948_888,
            is_live: true,
            title: "和菠萝双排亚服".to_string(),
            live_url: "https://live.bilibili.com/8948888".to_string(),
            cover_url: None,
            live_started_at: Some("2026-03-16 20:54:17".to_string()),
        };

        assert_eq!(
            build_sub_reply(true, &streamer, Some(&status)),
            "已订阅 JDG-mmonk1(44235058)\n当前正在直播\n标题：和菠萝双排亚服\n链接：https://live.bilibili.com/8948888"
        );
    }

    #[test]
    fn snapshot_keeps_current_live_status() {
        let streamer = ResolveStreamer {
            uid: 42,
            uname: "主播A".to_string(),
            room_id: 9_001,
        };
        let status = LiveStatus {
            room_id: 9_001,
            is_live: true,
            title: "测试直播".to_string(),
            live_url: "https://live.bilibili.com/9001".to_string(),
            cover_url: Some("https://example.com/cover.jpg".to_string()),
            live_started_at: Some("2026-03-16T10:00:00Z".to_string()),
        };

        let snapshot = streamer_state_snapshot(&streamer, &status);

        assert_eq!(snapshot.uid, 42);
        assert_eq!(snapshot.room_id, Some(9_001));
        assert_eq!(snapshot.uname.as_deref(), Some("主播A"));
        assert!(snapshot.is_live);
        assert_eq!(snapshot.title.as_deref(), Some("测试直播"));
        assert_eq!(
            snapshot.live_url.as_deref(),
            Some("https://live.bilibili.com/9001")
        );
        assert_eq!(
            snapshot.cover_url.as_deref(),
            Some("https://example.com/cover.jpg")
        );
        assert_eq!(
            snapshot.last_live_started_at.as_deref(),
            Some("2026-03-16T10:00:00Z")
        );
        assert!(snapshot.last_seen_at.is_some());
    }
}
