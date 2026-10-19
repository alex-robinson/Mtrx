//! End-to-end encryption: recovery (key backup + secret storage) and
//! interactive emoji verification between our own devices.

use std::sync::Arc;

use futures_util::{Stream, StreamExt};
use matrix_sdk::{
    Client,
    encryption::{
        CrossSigningResetAuthType,
        recovery::RecoveryState,
        verification::{
            SasState, SasVerification, Verification as SdkVerification, VerificationRequest,
            VerificationRequestState,
        },
    },
    ruma::{
        DeviceId, api::client::uiaa,
        events::key::verification::request::ToDeviceKeyVerificationRequestEvent,
    },
};
use tokio::{
    sync::{Mutex, broadcast, watch},
    task::JoinHandle,
};
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tracing::warn;

use crate::{Error, OwnedDeviceId, OwnedUserId, Result, Session};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryStatus {
    /// Not determined yet (no sync so far).
    Unknown,
    /// Recovery is set up and this device has all secrets.
    Enabled,
    /// No recovery set up for the account.
    Disabled,
    /// Recovery exists but this device lacks the secrets; enter the recovery
    /// key or verify with another device.
    Incomplete,
}

/// Progress of a [`Verification`].
#[derive(Clone, Debug, PartialEq)]
pub enum VerificationState {
    /// Waiting for the other device.
    Waiting,
    /// Compare these with the other device, then call [`Verification::confirm`]
    /// or [`Verification::mismatch`].
    Emojis(Vec<VerificationEmoji>),
    /// We confirmed; waiting for the other device to confirm.
    Confirmed,
    /// Emojis matched; our devices are exchanging cross-signing keys. Both
    /// devices must stay online until [`VerificationState::Done`].
    SharingKeys,
    Done,
    Cancelled(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerificationEmoji {
    pub symbol: String,
    pub description: String,
}

/// An interactive emoji (SAS) verification with one of our other devices.
///
/// The protocol steps are driven in the background; frontends watch
/// [`Verification::states`] and answer with [`Verification::confirm`] or
/// [`Verification::mismatch`].
pub struct Verification {
    request: VerificationRequest,
    sas: Arc<Mutex<Option<SasVerification>>>,
    state: watch::Receiver<VerificationState>,
    task: JoinHandle<()>,
}

impl Drop for Verification {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Verification {
    fn new(client: Client, request: VerificationRequest) -> Self {
        let (tx, state) = watch::channel(VerificationState::Waiting);
        let sas = Arc::new(Mutex::new(None));
        let task = tokio::spawn(drive(client, request.clone(), sas.clone(), tx));
        Self {
            request,
            sas,
            state,
            task,
        }
    }

    pub fn other_user(&self) -> OwnedUserId {
        self.request.other_user_id().to_owned()
    }

    /// Accept an incoming request.
    pub async fn accept(&self) -> Result<()> {
        Ok(self.request.accept().await?)
    }

    pub fn current(&self) -> VerificationState {
        self.state.borrow().clone()
    }

    /// The current state, then every change.
    pub fn states(&self) -> impl Stream<Item = VerificationState> + use<> {
        WatchStream::new(self.state.clone())
    }

    /// The emojis match.
    pub async fn confirm(&self) -> Result<()> {
        Ok(self.active_sas().await?.confirm().await?)
    }

    /// The emojis differ: cancel and mark the attempt as suspicious.
    pub async fn mismatch(&self) -> Result<()> {
        Ok(self.active_sas().await?.mismatch().await?)
    }

    pub async fn cancel(&self) -> Result<()> {
        match self.sas.lock().await.as_ref() {
            Some(sas) => sas.cancel().await?,
            None => self.request.cancel().await?,
        }
        Ok(())
    }

    async fn active_sas(&self) -> Result<SasVerification> {
        self.sas
            .lock()
            .await
            .clone()
            .ok_or_else(|| Error::Verification("no emoji comparison in progress".into()))
    }
}

/// Run the request until it becomes an emoji verification, then run that.
async fn drive(
    client: Client,
    request: VerificationRequest,
    slot: Arc<Mutex<Option<SasVerification>>>,
    tx: watch::Sender<VerificationState>,
) {
    let mut changes = request.changes();
    let mut state = request.state();
    loop {
        match state {
            VerificationRequestState::Ready { .. } if request.we_started() => {
                match request.start_sas().await {
                    Ok(Some(sas)) => return drive_sas(client, sas, slot, tx).await,
                    Ok(None) => {
                        tx.send_replace(VerificationState::Cancelled(
                            "other device does not support emoji verification".into(),
                        ));
                        return;
                    }
                    Err(e) => {
                        tx.send_replace(VerificationState::Cancelled(e.to_string()));
                        return;
                    }
                }
            }
            VerificationRequestState::Transitioned {
                verification: SdkVerification::SasV1(sas),
            } => {
                return drive_sas(client, sas, slot, tx).await;
            }
            VerificationRequestState::Transitioned { .. } => {
                tx.send_replace(VerificationState::Cancelled("unsupported method".into()));
                return;
            }
            VerificationRequestState::Done => {
                tx.send_replace(VerificationState::Done);
                return;
            }
            VerificationRequestState::Cancelled(info) => {
                tx.send_replace(VerificationState::Cancelled(info.reason().to_owned()));
                return;
            }
            _ => {}
        }
        match changes.next().await {
            Some(next) => state = next,
            None => return,
        }
    }
}

async fn drive_sas(
    client: Client,
    sas: SasVerification,
    slot: Arc<Mutex<Option<SasVerification>>>,
    tx: watch::Sender<VerificationState>,
) {
    *slot.lock().await = Some(sas.clone());
    let mut changes = sas.changes();
    let mut state = sas.state();
    loop {
        match state {
            // The other device started: accept to move on to the key exchange.
            SasState::Started { .. } => {
                if let Err(e) = sas.accept().await {
                    tx.send_replace(VerificationState::Cancelled(e.to_string()));
                    return;
                }
            }
            SasState::KeysExchanged {
                emojis: Some(emojis),
                ..
            } => {
                let emojis = emojis
                    .emojis
                    .iter()
                    .map(|e| VerificationEmoji {
                        symbol: e.symbol.to_owned(),
                        description: e.description.to_owned(),
                    })
                    .collect();
                tx.send_replace(VerificationState::Emojis(emojis));
            }
            SasState::KeysExchanged { emojis: None, .. } => {
                let _ = sas.cancel().await;
                tx.send_replace(VerificationState::Cancelled(
                    "other device does not support emoji verification".into(),
                ));
                return;
            }
            SasState::Confirmed => {
                tx.send_replace(VerificationState::Confirmed);
            }
            SasState::Done { .. } => {
                if sas.other_user_id() == client.user_id().expect("logged in") {
                    tx.send_replace(VerificationState::SharingKeys);
                    let other = sas.other_device().device_id().to_owned();
                    if let Err(e) = wait_cross_signed(&client, &other).await {
                        tx.send_replace(VerificationState::Cancelled(e.to_string()));
                        return;
                    }
                }
                tx.send_replace(VerificationState::Done);
                return;
            }
            SasState::Cancelled(info) => {
                tx.send_replace(VerificationState::Cancelled(info.reason().to_owned()));
                return;
            }
            _ => {}
        }
        match changes.next().await {
            Some(next) => state = next,
            None => return,
        }
    }
}

/// After self-verification the new device fetches the cross-signing keys from
/// the other one and signs itself. Wait until both devices are cross-signed.
async fn wait_cross_signed(client: &Client, other: &DeviceId) -> Result<()> {
    let encryption = client.encryption();
    let user = client.user_id().expect("logged in");
    // Subscribe before checking so no update is missed.
    let mut updates = std::pin::pin!(encryption.devices_stream().await?);
    loop {
        let signed = |device: Option<matrix_sdk::encryption::identities::Device>| {
            device.is_some_and(|d| d.is_cross_signed_by_owner())
        };
        if signed(encryption.get_own_device().await?)
            && signed(encryption.get_device(user, other).await?)
        {
            return Ok(());
        }
        if updates.next().await.is_none() {
            return Err(Error::Verification("device updates ended".into()));
        }
    }
}

/// Forward incoming verification requests to [`Session::verification_requests`].
pub(crate) fn register_handlers(client: &Client) -> broadcast::Sender<VerificationRequest> {
    let (tx, _) = broadcast::channel(8);
    let sender = tx.clone();
    client.add_event_handler(
        move |event: ToDeviceKeyVerificationRequestEvent, client: Client| {
            let sender = sender.clone();
            async move {
                let request = client
                    .encryption()
                    .get_verification_request(&event.sender, &event.content.transaction_id)
                    .await;
                match request {
                    Some(request) => {
                        let _ = sender.send(request);
                    }
                    None => warn!(
                        "verification request {} not found",
                        event.content.transaction_id
                    ),
                }
            }
        },
    );
    tx
}

impl Session {
    pub fn recovery_status(&self) -> RecoveryStatus {
        match self.client.encryption().recovery().state() {
            RecoveryState::Unknown => RecoveryStatus::Unknown,
            RecoveryState::Enabled => RecoveryStatus::Enabled,
            RecoveryState::Disabled => RecoveryStatus::Disabled,
            RecoveryState::Incomplete => RecoveryStatus::Incomplete,
        }
    }

    /// Set up recovery: back up room keys and store our secrets on the server,
    /// protected by the returned recovery key. Show it to the user once.
    pub async fn enable_recovery(&self) -> Result<String> {
        Ok(self
            .client
            .encryption()
            .recovery()
            .enable()
            .wait_for_backups_to_upload()
            .await?)
    }

    /// Start over when neither another device nor the recovery key can unlock
    /// encryption: creates a new cross-signing identity and recovery, and
    /// returns the new recovery key. Messages only the old keys could decrypt
    /// stay unreadable, and other users see that our identity changed.
    pub async fn reset_encryption(&self, password: &str) -> Result<String> {
        let recovery = self.client.encryption().recovery();
        if let Some(handle) = recovery.reset_identity().await? {
            match handle.auth_type() {
                CrossSigningResetAuthType::Uiaa(uiaa) => {
                    let mut auth =
                        uiaa::Password::new((&self.user_id()).into(), password.to_owned());
                    auth.session = uiaa.session.clone();
                    handle.reset(Some(uiaa::AuthData::Password(auth))).await?;
                }
                CrossSigningResetAuthType::OAuth(_) => {
                    return Err(Error::Verification(
                        "identity reset via OAuth is not supported yet".into(),
                    ));
                }
            }
        }
        self.enable_recovery().await
    }

    /// Unlock recovery on this device: imports the cross-signing keys (this
    /// device becomes verified) and the key backup (old messages decrypt).
    pub async fn recover(&self, recovery_key: &str) -> Result<()> {
        Ok(self
            .client
            .encryption()
            .recovery()
            .recover(recovery_key)
            .await?)
    }

    /// Whether this device is verified (signed by our cross-signing identity).
    pub async fn is_device_verified(&self) -> Result<bool> {
        let device = self
            .client
            .encryption()
            .get_own_device()
            .await?
            .ok_or_else(|| Error::Verification("own device not found".into()))?;
        Ok(device.is_cross_signed_by_owner())
    }

    /// Ask our other devices to verify this one.
    pub async fn request_self_verification(&self) -> Result<Verification> {
        let identity = self
            .client
            .encryption()
            .get_user_identity(&self.user_id())
            .await?
            .ok_or_else(|| Error::Verification("account has no cross-signing identity".into()))?;
        let request = identity
            .request_verification()
            .await
            .map_err(|e| Error::Verification(e.to_string()))?;
        Ok(Verification::new(self.client.clone(), request))
    }

    /// Verification requests from our other devices. Sync must be running.
    pub fn verification_requests(&self) -> impl Stream<Item = Verification> + use<> {
        let client = self.client.clone();
        BroadcastStream::new(self.verification_tx.subscribe()).filter_map(move |r| {
            let client = client.clone();
            async move { r.ok().map(|request| Verification::new(client, request)) }
        })
    }

    pub async fn devices(&self) -> Result<Vec<DeviceInfo>> {
        let devices = self
            .client
            .encryption()
            .get_user_devices(&self.user_id())
            .await?;
        Ok(devices
            .devices()
            .map(|d| DeviceInfo {
                id: d.device_id().to_owned(),
                name: d.display_name().map(ToOwned::to_owned),
                verified: d.is_cross_signed_by_owner(),
                current: d.device_id() == self.device_id(),
            })
            .collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: OwnedDeviceId,
    pub name: Option<String>,
    pub verified: bool,
    pub current: bool,
}
