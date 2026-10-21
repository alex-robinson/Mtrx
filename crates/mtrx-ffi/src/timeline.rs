//! A room's live timeline, and sending.

use std::sync::Arc;

use futures_util::StreamExt;
use mtrx_core::RoomTimeline;

use crate::{
    MtrxError, TaskHandle, TimelineItem,
    runtime::{InRuntime, run},
    types::timeline_items,
};

/// Receives a timeline's items, oldest first: all of them, now and after
/// every change.
#[uniffi::export(with_foreign)]
pub trait TimelineListener: Send + Sync {
    fn on_update(&self, items: Vec<TimelineItem>);
}

/// A room's timeline, kept up to date while the object lives and sync runs.
#[derive(uniffi::Object)]
pub struct Timeline {
    inner: InRuntime<Arc<RoomTimeline>>,
}

impl Timeline {
    pub(crate) fn new(timeline: RoomTimeline) -> Arc<Self> {
        Arc::new(Self {
            inner: InRuntime::new(Arc::new(timeline)),
        })
    }

    fn core(&self) -> Arc<RoomTimeline> {
        Arc::clone(&self.inner)
    }
}

#[uniffi::export]
impl Timeline {
    pub fn room_id(&self) -> String {
        self.inner.room_id().to_string()
    }

    /// Call `listener` with the items, now and on every change. Ends when
    /// the timeline is dropped.
    pub fn subscribe(&self, listener: Arc<dyn TimelineListener>) -> Arc<TaskHandle> {
        let mut updates = Box::pin(self.inner.updates());
        TaskHandle::spawn(async move {
            while let Some(entries) = updates.next().await {
                listener.on_update(timeline_items(&entries));
            }
        })
    }

    /// Load up to `events` older events (when scrolled to the top). Returns
    /// `true` once the room's start is reached.
    pub async fn paginate_back(&self, events: u16) -> Result<bool, MtrxError> {
        let timeline = self.core();
        run(async move { Ok(timeline.paginate_back(events).await?) }).await
    }

    /// Send a message written in Markdown. It shows in the timeline at once,
    /// as [`crate::SendState::Sending`], and is sent in the background, also
    /// later if the server is unreachable now.
    pub async fn send_text(&self, text: String) -> Result<(), MtrxError> {
        let timeline = self.core();
        run(async move {
            timeline.send_markdown(&text, None).await?;
            Ok(())
        })
        .await
    }

    /// Mark everything up to the latest event as read.
    pub async fn mark_read(&self) -> Result<(), MtrxError> {
        let timeline = self.core();
        run(async move { Ok(timeline.mark_read().await?) }).await
    }
}
