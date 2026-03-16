use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use chrono::Utc;
use log::warn;

use ayiou::adapter::onebot::v11::model::{Message, MessageSegment};
use ayiou::core::plugin_host::MessageSender;

use crate::client::{LiveClient, LiveStatus};
use crate::model::{NotifyTarget, StreamerState};
use crate::repo::SubscriptionRepo;

#[derive(Clone)]
pub struct Poller {
    repo: SubscriptionRepo,
    client: Arc<dyn LiveClient>,
    sender: Arc<dyn MessageSender>,
    bootstrapped: Arc<AtomicBool>,
}

impl Poller {
    pub fn new(
        repo: SubscriptionRepo,
        client: Arc<dyn LiveClient>,
        sender: Arc<dyn MessageSender>,
    ) -> Self {
        Self {
            repo,
            client,
            sender,
            bootstrapped: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn poll_once(&self) -> Result<()> {
        // The first poll establishes the current live state after process start.
        let suppress_notifications = !self.bootstrapped.load(Ordering::SeqCst);
        let subscriptions = self.repo.subscriptions_by_uid().await?;

        for (uid, targets) in subscriptions {
            let resolved = match self.client.resolve_streamer(uid).await {
                Ok(value) => value,
                Err(err) => {
                    warn!("resolve streamer {} failed: {}", uid, err);
                    continue;
                }
            };

            let status = match self.client.fetch_live_status(resolved.room_id).await {
                Ok(value) => value,
                Err(err) => {
                    warn!("fetch live status {} failed: {}", uid, err);
                    continue;
                }
            };

            let previous = self.repo.get_streamer_state(uid).await?.unwrap_or_default();
            let next = StreamerState {
                uid,
                room_id: Some(resolved.room_id),
                uname: Some(resolved.uname.clone()),
                is_live: status.is_live,
                title: Some(status.title.clone()),
                live_url: Some(status.live_url.clone()),
                cover_url: status.cover_url.clone(),
                last_live_started_at: status.live_started_at.clone(),
                last_seen_at: Some(Utc::now().to_rfc3339()),
            };

            self.repo.set_streamer_state(&next).await?;

            if !suppress_notifications && !previous.is_live && status.is_live {
                let message = build_live_notification_message(&resolved.uname, &status);

                for target in targets {
                    match target {
                        NotifyTarget::Group(group_id) => {
                            if let Err(err) = self
                                .sender
                                .send_group_message(group_id, message.clone())
                                .await
                            {
                                warn!("send group live notification failed: {}", err);
                            }
                        }
                        NotifyTarget::Private(user_id) => {
                            if let Err(err) = self
                                .sender
                                .send_private_message(user_id, message.clone())
                                .await
                            {
                                warn!("send private live notification failed: {}", err);
                            }
                        }
                    }
                }
            }
        }

        self.bootstrapped.store(true, Ordering::SeqCst);

        Ok(())
    }
}

fn build_live_notification_message(uname: &str, status: &LiveStatus) -> Message {
    let text = build_live_notification_text(uname, status);
    match status.cover_url.as_deref().filter(|url| !url.is_empty()) {
        Some(cover_url) => Message::Array(vec![
            MessageSegment::Text { text },
            MessageSegment::Image {
                file: cover_url.to_string(),
                image_type: None,
                url: None,
            },
        ]),
        None => Message::String(text),
    }
}

fn build_live_notification_text(uname: &str, status: &LiveStatus) -> String {
    format!(
        "{} 开播了\n标题：{}\n链接：{}",
        uname, status.title, status.live_url
    )
}

#[cfg(test)]
mod tests {
    use super::{build_live_notification_message, build_live_notification_text};
    use ayiou::adapter::onebot::v11::model::{Message, MessageSegment};

    use crate::client::LiveStatus;

    #[test]
    fn live_notification_text_has_title_and_link() {
        let status = LiveStatus {
            room_id: 9_001,
            is_live: true,
            title: "测试直播".to_string(),
            live_url: "https://live.bilibili.com/9001".to_string(),
            cover_url: None,
            live_started_at: Some("2026-03-16T10:00:00Z".to_string()),
        };

        assert_eq!(
            build_live_notification_text("主播A", &status),
            "主播A 开播了\n标题：测试直播\n链接：https://live.bilibili.com/9001"
        );
    }

    #[test]
    fn live_notification_message_uses_cover_image_when_present() {
        let status = LiveStatus {
            room_id: 9_001,
            is_live: true,
            title: "测试直播".to_string(),
            live_url: "https://live.bilibili.com/9001".to_string(),
            cover_url: Some("https://example.com/cover.jpg".to_string()),
            live_started_at: Some("2026-03-16T10:00:00Z".to_string()),
        };

        match build_live_notification_message("主播A", &status) {
            Message::Array(segments) => {
                assert!(matches!(
                    &segments[0],
                    MessageSegment::Text { text }
                    if text == "主播A 开播了\n标题：测试直播\n链接：https://live.bilibili.com/9001"
                ));
                assert!(matches!(
                    &segments[1],
                    MessageSegment::Image { file, .. } if file == "https://example.com/cover.jpg"
                ));
            }
            other => panic!("expected image message, got {other:?}"),
        }
    }
}
