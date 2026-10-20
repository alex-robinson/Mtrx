//! Whether the server is reachable. Sync goes offline on network errors and
//! checks the server until it answers again; meanwhile messages wait in the
//! send queue, which is re-enabled once the server is back.

use std::{sync::Arc, time::Duration};

use matrix_sdk::Client;
use matrix_sdk_ui::sync_service::{State, SyncService};
use tokio::{
    sync::{broadcast, watch},
    time::Instant,
};
use tokio_stream::{Stream, wrappers::WatchStream};
use tracing::warn;

use crate::{Session, session::AbortOnDrop};

/// After an unexpected sync error, how long until sync starts again.
const RESTART: Duration = Duration::from_secs(5);
/// After a send failed although sync runs, how long until it is retried.
const RESEND: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Connection {
    #[default]
    Online,
    /// The server is unreachable; sync checks it until it answers.
    Offline,
}

/// Follow sync's state as a [`Connection`] until dropped, restarting sync
/// after errors and resuming sends once the server is reachable.
pub(crate) fn watch(
    client: Client,
    sync: Arc<SyncService>,
) -> (watch::Receiver<Connection>, AbortOnDrop) {
    let (tx, rx) = watch::channel(Connection::Online);
    let task = tokio::spawn(async move {
        let mut states = sync.state();
        let mut errors = client.send_queue().subscribe_errors();
        let mut resend: Option<Instant> = None;
        loop {
            tokio::select! {
                state = states.next() => match state {
                    Some(State::Running) => {
                        tx.send_replace(Connection::Online);
                        // Sends stop on network errors; the server is back.
                        client.send_queue().set_enabled(true).await;
                    }
                    Some(State::Offline) => {
                        tx.send_replace(Connection::Offline);
                    }
                    Some(State::Error(e)) => {
                        warn!("sync failed, restarting: {e}");
                        tx.send_replace(Connection::Offline);
                        tokio::time::sleep(RESTART).await;
                        sync.start().await;
                    }
                    Some(State::Idle | State::Terminated) => {}
                    None => return,
                },
                error = errors.recv() => match error {
                    // A send failed while sync works: retry it a bit later.
                    Ok(error) if error.is_recoverable => {
                        resend.get_or_insert(Instant::now() + RESEND);
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                _ = tokio::time::sleep_until(resend.unwrap_or_else(Instant::now)),
                    if resend.is_some() =>
                {
                    resend = None;
                    if *tx.borrow() == Connection::Online {
                        client.send_queue().set_enabled(true).await;
                    }
                }
            }
        }
    });
    (rx, AbortOnDrop(task))
}

impl Session {
    /// Whether the server is reachable, as far as sync knows.
    pub fn connection(&self) -> Connection {
        *self.connection.borrow()
    }

    /// The connection, then every change.
    pub fn connection_updates(&self) -> impl Stream<Item = Connection> + use<> {
        WatchStream::new(self.connection.clone())
    }

    /// Try the server now instead of waiting for the next check.
    pub async fn reconnect(&self) {
        self.sync.start().await;
    }
}
