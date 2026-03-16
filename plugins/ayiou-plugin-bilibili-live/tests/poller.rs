use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;

use ayiou::core::plugin_host::MessageSender;
use ayiou::core::storage::MemoryStore;
use ayiou_plugin_bilibili_live::client::{LiveClient, LiveStatus, ResolveStreamer};
use ayiou_plugin_bilibili_live::model::StreamerState;
use ayiou_plugin_bilibili_live::poller::Poller;
use ayiou_plugin_bilibili_live::repo::SubscriptionRepo;

#[derive(Default, Clone)]
struct RecordingSender {
    group_messages: Arc<Mutex<Vec<(i64, String)>>>,
}

impl RecordingSender {
    fn group_messages(&self) -> Vec<(i64, String)> {
        self.group_messages.lock().unwrap().clone()
    }
}

#[async_trait]
impl MessageSender for RecordingSender {
    async fn send_private_text(&self, _user_id: i64, _text: &str) -> Result<()> {
        Ok(())
    }

    async fn send_group_text(&self, group_id: i64, text: &str) -> Result<()> {
        self.group_messages
            .lock()
            .unwrap()
            .push((group_id, text.to_string()));
        Ok(())
    }
}

struct FakeClient {
    states: Mutex<VecDeque<LiveStatus>>,
}

#[async_trait]
impl LiveClient for FakeClient {
    async fn resolve_streamer(&self, uid: u64) -> Result<ResolveStreamer> {
        Ok(ResolveStreamer {
            uid,
            uname: "主播A".to_string(),
            room_id: 9001,
        })
    }

    async fn fetch_live_status(&self, _room_id: u64) -> Result<LiveStatus> {
        let mut states = self.states.lock().unwrap();
        Ok(states.pop_front().expect("missing fake state"))
    }
}

#[tokio::test]
async fn poller_only_notifies_on_live_edge() {
    let store = Arc::new(MemoryStore::new());
    let repo = SubscriptionRepo::new(store);
    repo.subscribe_group(123, 42).await.unwrap();

    let client = Arc::new(FakeClient {
        states: Mutex::new(VecDeque::from(vec![
            LiveStatus {
                room_id: 9001,
                is_live: false,
                title: "测试直播".to_string(),
                live_url: "https://live.bilibili.com/9001".to_string(),
                cover_url: None,
                live_started_at: None,
            },
            LiveStatus {
                room_id: 9001,
                is_live: true,
                title: "测试直播".to_string(),
                live_url: "https://live.bilibili.com/9001".to_string(),
                cover_url: None,
                live_started_at: Some("2026-03-16T10:00:00Z".to_string()),
            },
            LiveStatus {
                room_id: 9001,
                is_live: true,
                title: "测试直播".to_string(),
                live_url: "https://live.bilibili.com/9001".to_string(),
                cover_url: None,
                live_started_at: Some("2026-03-16T10:00:00Z".to_string()),
            },
        ])),
    });
    let sender = Arc::new(RecordingSender::default());
    let poller = Poller::new(repo, client, sender.clone());

    poller.poll_once().await.unwrap();
    poller.poll_once().await.unwrap();
    poller.poll_once().await.unwrap();

    assert_eq!(
        sender.group_messages(),
        vec![(
            123,
            "主播A 开播了\n标题：测试直播\n链接：https://live.bilibili.com/9001".to_string(),
        )]
    );
}

#[tokio::test]
async fn poller_skips_notification_when_streamer_is_already_live() {
    let store = Arc::new(MemoryStore::new());
    let repo = SubscriptionRepo::new(store);
    repo.subscribe_group(123, 42).await.unwrap();
    repo.set_streamer_state(&StreamerState {
        uid: 42,
        room_id: Some(9001),
        uname: Some("主播A".to_string()),
        is_live: true,
        title: Some("测试直播".to_string()),
        live_url: Some("https://live.bilibili.com/9001".to_string()),
        cover_url: None,
        last_live_started_at: Some("2026-03-16T10:00:00Z".to_string()),
        last_seen_at: Some("2026-03-16T10:00:05Z".to_string()),
    })
    .await
    .unwrap();

    let client = Arc::new(FakeClient {
        states: Mutex::new(VecDeque::from(vec![LiveStatus {
            room_id: 9001,
            is_live: true,
            title: "测试直播".to_string(),
            live_url: "https://live.bilibili.com/9001".to_string(),
            cover_url: None,
            live_started_at: Some("2026-03-16T10:00:00Z".to_string()),
        }])),
    });
    let sender = Arc::new(RecordingSender::default());
    let poller = Poller::new(repo, client, sender.clone());

    poller.poll_once().await.unwrap();

    assert!(sender.group_messages().is_empty());
}
