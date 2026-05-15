use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    sync::{Arc, RwLock},
};

use anyhow::{Result, anyhow};
use tokio::sync::broadcast;

use crate::core::service::RuntimeService;

pub trait RuntimeEvent: Clone + Send + Sync + 'static {
    fn topic() -> &'static str;
}

#[derive(Clone)]
pub struct RuntimeEventBus {
    capacity: usize,
    topics: Arc<RwLock<HashMap<TypeId, EventTopic>>>,
}

struct EventTopic {
    topic: &'static str,
    sender: Box<dyn Any + Send + Sync>,
}

impl RuntimeEventBus {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            topics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn publish<E>(&self, event: E) -> Result<usize>
    where
        E: RuntimeEvent,
    {
        let sender = self.sender::<E>()?;
        if sender.receiver_count() == 0 {
            return Ok(0);
        }
        sender
            .send(event)
            .map_err(|err| anyhow!("runtime event `{}` publish failed: {err}", E::topic()))
    }

    pub fn subscribe<E>(&self) -> broadcast::Receiver<E>
    where
        E: RuntimeEvent,
    {
        self.sender::<E>()
            .expect("runtime event sender should match event type")
            .subscribe()
    }

    fn sender<E>(&self) -> Result<broadcast::Sender<E>>
    where
        E: RuntimeEvent,
    {
        let mut topics = self.topics.write().expect("event bus topics lock");
        let topic = topics.entry(TypeId::of::<E>()).or_insert_with(|| {
            let (sender, _rx) = broadcast::channel::<E>(self.capacity);
            EventTopic {
                topic: E::topic(),
                sender: Box::new(sender),
            }
        });

        topic
            .sender
            .downcast_ref::<broadcast::Sender<E>>()
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "runtime event `{}` sender type mismatch for `{}`",
                    topic.topic,
                    type_name::<E>()
                )
            })
    }
}

impl RuntimeService for RuntimeEventBus {
    fn name(&self) -> &'static str {
        "runtime-event-bus"
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::broadcast;

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TestEvent {
        value: usize,
    }

    impl RuntimeEvent for TestEvent {
        fn topic() -> &'static str {
            "test-event"
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct OtherEvent;

    impl RuntimeEvent for OtherEvent {
        fn topic() -> &'static str {
            "other-event"
        }
    }

    #[tokio::test]
    async fn event_bus_delivers_typed_events_to_subscribers() {
        let bus = RuntimeEventBus::new(8);
        let mut rx = bus.subscribe::<TestEvent>();

        let subscribers = bus.publish(TestEvent { value: 7 }).unwrap();
        let event = rx.recv().await.unwrap();

        assert_eq!(subscribers, 1);
        assert_eq!(event, TestEvent { value: 7 });
    }

    #[tokio::test]
    async fn event_bus_is_scoped_to_one_runtime() {
        let first = RuntimeEventBus::new(8);
        let second = RuntimeEventBus::new(8);
        let mut rx = first.subscribe::<TestEvent>();

        let subscribers = second.publish(TestEvent { value: 7 }).unwrap();

        assert_eq!(subscribers, 0);
        assert!(matches!(
            rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn event_bus_reports_lag_for_slow_subscribers() {
        let bus = RuntimeEventBus::new(1);
        let mut rx = bus.subscribe::<TestEvent>();

        bus.publish(TestEvent { value: 1 }).unwrap();
        bus.publish(TestEvent { value: 2 }).unwrap();

        assert!(matches!(
            rx.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
        assert_eq!(rx.recv().await.unwrap(), TestEvent { value: 2 });
    }

    #[tokio::test]
    async fn event_bus_keeps_event_types_isolated_by_topic() {
        let bus = RuntimeEventBus::new(8);
        let mut test_rx = bus.subscribe::<TestEvent>();
        let mut other_rx = bus.subscribe::<OtherEvent>();

        bus.publish(TestEvent { value: 7 }).unwrap();

        assert_eq!(test_rx.recv().await.unwrap(), TestEvent { value: 7 });
        assert!(matches!(
            other_rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }
}
