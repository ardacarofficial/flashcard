//! Event bus (docs/03 §8). Commands publish domain events **after commit**; subscribers are the
//! frontend bridge, automations, plugins and caches.
//!
//! Hooks (`before_*` may modify/reject, `after_*` react) arrive with plugins in v0.7 and will
//! wrap the dispatcher, not this bus.

use fc_api::v1::events::EventEnvelope;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// Callback receiving events. Must be fast and must not block (hand off to a channel).
pub type EventSink = Arc<dyn Fn(&EventEnvelope) + Send + Sync>;

/// Subscription handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

/// Publish/subscribe for domain events.
pub trait EventBus: Send + Sync {
    /// Delivers `event` to every subscriber (in subscription order). A panicking subscriber
    /// must not affect others (implementations isolate failures where the platform allows).
    fn publish(&self, event: &EventEnvelope);
    /// Adds a subscriber.
    fn subscribe(&self, sink: EventSink) -> SubscriptionId;
    /// Removes a subscriber. Unknown ids are ignored.
    fn unsubscribe(&self, id: SubscriptionId);
}

/// Synchronous in-process bus.
#[derive(Default)]
pub struct InMemoryEventBus {
    subscribers: RwLock<Vec<(SubscriptionId, EventSink)>>,
    next_id: AtomicU64,
}

impl InMemoryEventBus {
    /// Creates an empty bus.
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventBus for InMemoryEventBus {
    fn publish(&self, event: &EventEnvelope) {
        let subs: Vec<EventSink> = match self.subscribers.read() {
            Ok(s) => s.iter().map(|(_, f)| f.clone()).collect(),
            Err(poisoned) => poisoned
                .into_inner()
                .iter()
                .map(|(_, f)| f.clone())
                .collect(),
        };
        for sink in subs {
            sink(event);
        }
    }

    fn subscribe(&self, sink: EventSink) -> SubscriptionId {
        let id = SubscriptionId(self.next_id.fetch_add(1, Ordering::Relaxed));
        match self.subscribers.write() {
            Ok(mut s) => s.push((id, sink)),
            Err(poisoned) => poisoned.into_inner().push((id, sink)),
        }
        id
    }

    fn unsubscribe(&self, id: SubscriptionId) {
        match self.subscribers.write() {
            Ok(mut s) => s.retain(|(i, _)| *i != id),
            Err(poisoned) => poisoned.into_inner().retain(|(i, _)| *i != id),
        }
    }
}
