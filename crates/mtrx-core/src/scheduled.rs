//! Send later: messages kept on this device and sent when their time comes.
//!
//! Matrix has no server-side scheduling for ordinary messages, so scheduled
//! messages live in the local (encrypted) store, and a scheduler task hands
//! them to the send queue when due. That only happens while a scheduler runs
//! (the app, `mtrx watch`); a message whose time passed while none ran is
//! held as overdue until the user sends or cancels it.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::Stream;
use matrix_sdk::{
    Client,
    ruma::{OwnedRoomId, RoomId},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, watch};
use tokio_stream::wrappers::WatchStream;
use tracing::warn;

use crate::{Error, PendingSend, Result, Session, session::AbortOnDrop, timeline::send_markdown};

/// Store key of the list of scheduled messages.
const STORE_KEY: &[u8] = b"mtrx.scheduled_messages";
/// A message found this late is held instead of sent: the app was probably
/// closed or the computer asleep at its time.
const GRACE: Duration = Duration::from_secs(5 * 60);
/// Longest wait between checks. Timers may not count time the computer
/// sleeps, so the wall clock is checked regularly.
const MAX_WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledMessage {
    pub id: String,
    pub room_id: OwnedRoomId,
    /// Markdown, as written.
    pub text: String,
    #[serde(with = "millis")]
    pub send_at: SystemTime,
    /// Its time passed while no scheduler ran; it waits for the user to
    /// send, reschedule or cancel it.
    pub overdue: bool,
}

/// The scheduled messages of a session, persisted on every change.
pub(crate) struct Schedule {
    client: Client,
    send_lock: Arc<Mutex<()>>,
    messages: watch::Sender<Vec<ScheduledMessage>>,
    /// Serializes changes so each is persisted before the next starts.
    write: Mutex<()>,
}

impl Schedule {
    pub(crate) async fn load(client: Client, send_lock: Arc<Mutex<()>>) -> Result<Self> {
        let messages = match client.state_store().get_custom_value(STORE_KEY).await? {
            Some(bytes) => serde_json::from_slice(&bytes)?,
            None => Vec::new(),
        };
        Ok(Self {
            client,
            send_lock,
            messages: watch::Sender::new(messages),
            write: Mutex::new(()),
        })
    }

    /// Apply `change` to the list and persist it if it changed; nothing
    /// changes if `change` fails.
    async fn modify<R>(
        &self,
        change: impl AsyncFnOnce(&mut Vec<ScheduledMessage>) -> Result<R>,
    ) -> Result<R> {
        let _guard = self.write.lock().await;
        let mut messages = self.messages.borrow().clone();
        let result = change(&mut messages).await?;
        messages.sort_by_key(|m| m.send_at);
        if messages != *self.messages.borrow() {
            self.client
                .state_store()
                .set_custom_value(STORE_KEY, serde_json::to_vec(&messages)?)
                .await?;
            self.messages.send_replace(messages);
        }
        Ok(result)
    }

    /// Send what is due, hold what is late. Returns when the next message is due.
    async fn dispatch(&self, now: SystemTime, grace: Duration) -> Result<Option<SystemTime>> {
        self.modify(async |messages| {
            let mut kept = Vec::new();
            for mut message in std::mem::take(messages) {
                if message.overdue || message.send_at > now {
                    kept.push(message);
                } else if now.duration_since(message.send_at).unwrap_or_default() > grace {
                    message.overdue = true;
                    kept.push(message);
                } else if let Err(e) = self.send(&message).await {
                    warn!("could not send scheduled message: {e}");
                    message.overdue = true;
                    kept.push(message);
                }
            }
            *messages = kept;
            Ok(messages
                .iter()
                .filter(|m| !m.overdue)
                .map(|m| m.send_at)
                .min())
        })
        .await
    }

    /// Hand a message to the send queue, which persists it and sends it as
    /// soon as the server is reachable.
    async fn send(&self, message: &ScheduledMessage) -> Result<PendingSend> {
        let room = self
            .client
            .get_room(&message.room_id)
            .ok_or_else(|| Error::RoomNotFound(message.room_id.to_string()))?;
        send_markdown(&room, &self.send_lock, &message.text).await
    }
}

fn position(messages: &[ScheduledMessage], id: &str) -> Result<usize> {
    messages
        .iter()
        .position(|m| m.id == id)
        .ok_or_else(|| Error::ScheduledNotFound(id.to_owned()))
}

impl Session {
    /// Scheduled messages of all rooms, earliest first.
    pub fn scheduled(&self) -> Vec<ScheduledMessage> {
        self.schedule.messages.borrow().clone()
    }

    /// The scheduled messages, then every change.
    pub fn scheduled_updates(&self) -> impl Stream<Item = Vec<ScheduledMessage>> + use<> {
        WatchStream::new(self.schedule.messages.subscribe())
    }

    /// Schedule a Markdown message for `send_at`, which must be in the future.
    pub async fn schedule(
        &self,
        room_id: &RoomId,
        text: &str,
        send_at: SystemTime,
    ) -> Result<ScheduledMessage> {
        self.room(room_id)?;
        let send_at = future_time(send_at)?;
        let message = ScheduledMessage {
            id: random_id(),
            room_id: room_id.to_owned(),
            text: text.to_owned(),
            send_at,
            overdue: false,
        };
        self.schedule
            .modify(async |messages| {
                messages.push(message.clone());
                Ok(())
            })
            .await?;
        Ok(message)
    }

    /// Change a scheduled message's text and time; `send_at` must be in the
    /// future. An overdue message waits again.
    pub async fn reschedule(&self, id: &str, text: &str, send_at: SystemTime) -> Result<()> {
        let send_at = future_time(send_at)?;
        self.schedule
            .modify(async |messages| {
                let index = position(messages, id)?;
                let message = &mut messages[index];
                message.text = text.to_owned();
                message.send_at = send_at;
                message.overdue = false;
                Ok(())
            })
            .await
    }

    /// Remove a scheduled message without sending it.
    pub async fn unschedule(&self, id: &str) -> Result<ScheduledMessage> {
        self.schedule
            .modify(async |messages| {
                let index = position(messages, id)?;
                Ok(messages.remove(index))
            })
            .await
    }

    /// Send a scheduled message now, e.g. an overdue one.
    pub async fn send_scheduled_now(&self, id: &str) -> Result<PendingSend> {
        let message = self.unschedule(id).await?;
        match self.schedule.send(&message).await {
            Ok(pending) => Ok(pending),
            Err(e) => {
                // Keep it rather than lose it.
                self.schedule
                    .modify(async |messages| {
                        messages.push(message);
                        Ok(())
                    })
                    .await?;
                Err(e)
            }
        }
    }

    /// Send scheduled messages when they are due, until the session is
    /// dropped. Messages whose time already passed are held as overdue.
    pub fn start_scheduler(&self) {
        self.start_scheduler_with_grace(GRACE);
    }

    /// [`Session::start_scheduler`] with a different lateness limit, for tests.
    #[cfg(feature = "test-support")]
    pub fn start_scheduler_for_test(&self, grace: Duration) {
        self.start_scheduler_with_grace(grace);
    }

    fn start_scheduler_with_grace(&self, grace: Duration) {
        let schedule = self.schedule.clone();
        let task = tokio::spawn(async move {
            let mut changes = schedule.messages.subscribe();
            loop {
                changes.mark_unchanged();
                let now = SystemTime::now();
                let wait = match schedule.dispatch(now, grace).await {
                    Ok(next) => next
                        .map(|at| at.duration_since(now).unwrap_or_default())
                        .map_or(MAX_WAIT, |wait| wait.min(MAX_WAIT)),
                    Err(e) => {
                        warn!("scheduler: {e}");
                        MAX_WAIT
                    }
                };
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = changes.changed() => {}
                }
            }
        });
        *self.scheduler.lock().expect("not poisoned") = Some(AbortOnDrop(task));
    }
}

/// `send_at` to the stored precision, if it is in the future.
fn future_time(send_at: SystemTime) -> Result<SystemTime> {
    if send_at <= SystemTime::now() {
        return Err(Error::SendTimeInPast);
    }
    let millis = send_at
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Ok(UNIX_EPOCH + Duration::from_millis(millis as u64))
}

fn random_id() -> String {
    let bytes: [u8; 4] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `SystemTime` as milliseconds since the Unix epoch.
mod millis {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(time: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let millis = time
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        s.serialize_u64(millis as u64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        Ok(UNIX_EPOCH + Duration::from_millis(u64::deserialize(d)?))
    }
}
