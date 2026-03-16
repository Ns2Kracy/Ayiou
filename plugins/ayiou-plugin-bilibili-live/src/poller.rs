use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use log::warn;

use ayiou::core::plugin_host::MessageSender;

use crate::client::LiveClient;
use crate::model::{NotifyTarget, StreamerState};
use crate::repo::SubscriptionRepo;

#[derive(Clone)]
pub struct Poller {
    repo: SubscriptionRepo,
    client: Arc<dyn LiveClient>,
    sender: Arc<dyn MessageSender>,
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
        }
    }

    pub async fn poll_once(&self) -> Result<()> {
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

            if !previous.is_live && status.is_live {
                let text = format!(
                    "{} 开播了\n标题：{}\n链接：{}",
                    resolved.uname, status.title, status.live_url
                );

                for target in targets {
                    match target {
                        NotifyTarget::Group(group_id) => {
                            if let Err(err) = self.sender.send_group_text(group_id, &text).await {
                                warn!("send group live notification failed: {}", err);
                            }
                        }
                        NotifyTarget::Private(user_id) => {
                            if let Err(err) = self.sender.send_private_text(user_id, &text).await {
                                warn!("send private live notification failed: {}", err);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
