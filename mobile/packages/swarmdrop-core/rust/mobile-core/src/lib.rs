//! UniFFI bridge for SwarmDrop mobile.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use swarm_p2p_core::EventReceiver;
use swarm_p2p_core::libp2p::{Multiaddr, PeerId};
use swarmdrop_core::device::{ConnectionType, Device, DeviceStatus, PairedDeviceInfo};
use swarmdrop_core::device_manager::DeviceFilter;
use swarmdrop_core::host::{
    CoreEvent, DeviceIdentityBytes, EventBus, IdentityMigrationState, KeychainProvider,
};
use swarmdrop_core::network::{NetManager, NetworkStatus as CoreNetworkStatus, NodeStatus};
use swarmdrop_core::pairing::code::ShareCodeRecord;
use swarmdrop_core::protocol::AppRequest;
use swarmdrop_core::protocol::{PairingMethod, PairingRefuseReason, PairingResponse};
use swarmdrop_core::transfer::incoming::{TransferOfferEvent, TransferOfferFileEvent};
use swarmdrop_core::{AppError, AppResult};
use tokio::sync::Mutex;
use uuid::Uuid;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("io error: {0}")]
    Io(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("identity error: {0}")]
    Identity(String),
    #[error("node not started")]
    NodeNotStarted,
    #[error("pairing code expired")]
    ExpiredCode,
    #[error("invalid pairing code")]
    InvalidCode,
    #[error("transfer error: {0}")]
    Transfer(String),
    #[error("database error: {0}")]
    Database(String),
}

impl From<AppError> for FfiError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Io(error) => Self::Io(error.to_string()),
            AppError::Serialization(error) => Self::Serialization(error.to_string()),
            AppError::P2p(error) => Self::Network(error.to_string()),
            AppError::Network(message) => Self::Network(message),
            AppError::Identity(message) => Self::Identity(message),
            AppError::NodeNotStarted => Self::NodeNotStarted,
            AppError::ExpiredCode => Self::ExpiredCode,
            AppError::InvalidCode => Self::InvalidCode,
            AppError::TaskJoin(error) => Self::Network(error.to_string()),
            AppError::Transfer(message) => Self::Transfer(message),
            AppError::Database(error) => Self::Database(error.to_string()),
        }
    }
}

impl From<FfiError> for AppError {
    fn from(error: FfiError) -> Self {
        match error {
            FfiError::Io(message) => AppError::Io(std::io::Error::other(message)),
            FfiError::Serialization(message) => AppError::Identity(message),
            FfiError::Network(message) => AppError::Network(message),
            FfiError::Identity(message) => AppError::Identity(message),
            FfiError::NodeNotStarted => AppError::NodeNotStarted,
            FfiError::ExpiredCode => AppError::ExpiredCode,
            FfiError::InvalidCode => AppError::InvalidCode,
            FfiError::Transfer(message) => AppError::Transfer(message),
            FfiError::Database(message) => AppError::Identity(message),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileIdentity {
    pub peer_id: String,
    pub created: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileNetworkStatus {
    pub status: String,
    pub peer_id: Option<String>,
    pub listen_addrs: Vec<String>,
    pub nat_status: String,
    pub public_addr: Option<String>,
    pub connected_peers: u64,
    pub discovered_peers: u64,
    pub relay_ready: bool,
    pub relay_peers: Vec<String>,
    pub bootstrap_connected: bool,
}

impl From<CoreNetworkStatus> for MobileNetworkStatus {
    fn from(status: CoreNetworkStatus) -> Self {
        Self {
            status: match status.status {
                NodeStatus::Running => "running".to_string(),
                NodeStatus::Stopped => "stopped".to_string(),
            },
            peer_id: status.peer_id.map(|peer_id| peer_id.to_string()),
            listen_addrs: status
                .listen_addrs
                .into_iter()
                .map(|addr| addr.to_string())
                .collect(),
            nat_status: format!("{:?}", status.nat_status),
            public_addr: status.public_addr.map(|addr| addr.to_string()),
            connected_peers: status.connected_peers as u64,
            discovered_peers: status.discovered_peers as u64,
            relay_ready: status.relay_ready,
            relay_peers: status
                .relay_peers
                .into_iter()
                .map(|peer_id| peer_id.to_string())
                .collect(),
            bootstrap_connected: status.bootstrap_connected,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePairingCode {
    pub code: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileRemoteDeviceInfo {
    pub peer_id: String,
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub arch: String,
    pub listen_addrs: Vec<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

impl MobileRemoteDeviceInfo {
    fn from_record(peer_id: PeerId, record: ShareCodeRecord) -> Self {
        Self {
            peer_id: peer_id.to_string(),
            hostname: record.os_info.hostname,
            os: record.os_info.os,
            platform: record.os_info.platform,
            arch: record.os_info.arch,
            listen_addrs: record
                .listen_addrs
                .into_iter()
                .map(|addr| addr.to_string())
                .collect(),
            created_at: record.created_at,
            expires_at: record.expires_at,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePairingResult {
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileDevice {
    pub peer_id: String,
    pub hostname: String,
    pub os: String,
    pub platform: String,
    pub arch: String,
    pub status: String,
    pub connection: Option<String>,
    pub latency_ms: Option<u64>,
    pub is_paired: bool,
}

impl From<Device> for MobileDevice {
    fn from(device: Device) -> Self {
        Self {
            peer_id: device.peer_id.to_string(),
            hostname: device.os_info.hostname,
            os: device.os_info.os,
            platform: device.os_info.platform,
            arch: device.os_info.arch,
            status: match device.status {
                DeviceStatus::Online => "online".to_string(),
                DeviceStatus::Offline => "offline".to_string(),
            },
            connection: device.connection.map(|connection| match connection {
                ConnectionType::Lan => "lan".to_string(),
                ConnectionType::Dcutr => "dcutr".to_string(),
                ConnectionType::Relay => "relay".to_string(),
            }),
            latency_ms: device.latency,
            is_paired: device.is_paired,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferFile {
    pub file_id: String,
    pub name: String,
    pub relative_path: Option<String>,
    pub uri: String,
    pub size: u64,
    pub is_directory: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferOfferFile {
    pub file_id: String,
    pub name: String,
    pub relative_path: Option<String>,
    pub size: u64,
    pub is_directory: bool,
}

impl From<TransferOfferFileEvent> for MobileTransferOfferFile {
    fn from(file: TransferOfferFileEvent) -> Self {
        Self {
            file_id: file.file_id.to_string(),
            name: file.name,
            relative_path: if file.relative_path.is_empty() {
                None
            } else {
                Some(file.relative_path)
            },
            size: file.size,
            is_directory: file.is_directory,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferOffer {
    pub session_id: String,
    pub peer_id: String,
    pub device_name: String,
    pub files: Vec<MobileTransferOfferFile>,
    pub total_size: u64,
}

impl From<TransferOfferEvent> for MobileTransferOffer {
    fn from(offer: TransferOfferEvent) -> Self {
        Self {
            session_id: offer.session_id.to_string(),
            peer_id: offer.peer_id,
            device_name: offer.device_name,
            files: offer.files.into_iter().map(Into::into).collect(),
            total_size: offer.total_size,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePreparedTransfer {
    pub prepared_id: String,
    pub total_size: u64,
    pub files: Vec<MobileTransferFile>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTransferSession {
    pub session_id: String,
    pub peer_id: String,
    pub direction: String,
    pub status: String,
    pub total_size: u64,
    pub completed_size: u64,
    pub progress: f32,
}

#[derive(Default)]
struct MobileTransferState {
    prepared: HashMap<String, MobilePreparedTransfer>,
    sessions: HashMap<String, MobileTransferSession>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum MobileCoreEvent {
    NetworkStatusChanged { status: MobileNetworkStatus },
    DevicesChanged,
    PairingRequestReceived {
        peer_id: String,
        pending_id: u64,
        code: Option<String>,
    },
    PairingCompleted { peer_id: String },
    TransferProgress { session_id: String, progress: f32 },
    TransferOfferReceived { offer: MobileTransferOffer },
    TransferCompleted { session_id: String },
    TransferFailed { session_id: String, error: String },
    TransferPaused { session_id: String },
    TransferDbError { session_id: String, message: String },
    Error { message: String },
}

#[uniffi::export(with_foreign)]
#[async_trait]
pub trait ForeignKeychainProvider: Send + Sync {
    async fn load_identity(&self) -> Result<Option<Vec<u8>>, FfiError>;
    async fn save_identity(&self, keypair: Vec<u8>) -> Result<(), FfiError>;
    async fn delete_identity(&self) -> Result<(), FfiError>;
    async fn load_paired_devices_json(&self) -> Result<String, FfiError>;
    async fn save_paired_devices_json(&self, devices_json: String) -> Result<(), FfiError>;
}

#[uniffi::export(with_foreign)]
pub trait ForeignEventBus: Send + Sync {
    fn emit(&self, event: MobileCoreEvent);
}

struct MobileKeychainAdapter {
    foreign: Arc<dyn ForeignKeychainProvider>,
}

impl MobileKeychainAdapter {
    fn new(foreign: Arc<dyn ForeignKeychainProvider>) -> Self {
        Self { foreign }
    }
}

#[async_trait]
impl KeychainProvider for MobileKeychainAdapter {
    async fn load_identity(&self) -> AppResult<Option<DeviceIdentityBytes>> {
        self.foreign
            .load_identity()
            .await
            .map(|value| value.map(|keypair| DeviceIdentityBytes { keypair }))
            .map_err(Into::into)
    }

    async fn save_identity(&self, identity: DeviceIdentityBytes) -> AppResult<()> {
        self.foreign
            .save_identity(identity.keypair)
            .await
            .map_err(Into::into)
    }

    async fn delete_identity(&self) -> AppResult<()> {
        self.foreign.delete_identity().await.map_err(Into::into)
    }

    async fn load_migration_state(&self) -> AppResult<IdentityMigrationState> {
        Ok(IdentityMigrationState::Completed)
    }

    async fn save_migration_state(&self, _state: IdentityMigrationState) -> AppResult<()> {
        Ok(())
    }

    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>> {
        let value = self.foreign.load_paired_devices_json().await?;
        if value.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&value).map_err(AppError::Serialization)
    }

    async fn save_paired_devices(&self, devices: Vec<PairedDeviceInfo>) -> AppResult<()> {
        let value = serde_json::to_string(&devices).map_err(AppError::Serialization)?;
        self.foreign
            .save_paired_devices_json(value)
            .await
            .map_err(Into::into)
    }
}

struct MobileEventBusAdapter {
    foreign: Arc<dyn ForeignEventBus>,
}

impl MobileEventBusAdapter {
    fn new(foreign: Arc<dyn ForeignEventBus>) -> Self {
        Self { foreign }
    }
}

#[async_trait]
impl EventBus for MobileEventBusAdapter {
    async fn publish(&self, event: CoreEvent) -> AppResult<()> {
        self.foreign.emit(map_event(event));
        Ok(())
    }
}

fn map_event(event: CoreEvent) -> MobileCoreEvent {
    match event {
        CoreEvent::NetworkStatusChanged { status } => MobileCoreEvent::NetworkStatusChanged {
            status: status.into(),
        },
        CoreEvent::DevicesChanged { .. } => MobileCoreEvent::DevicesChanged,
        CoreEvent::PairingRequestReceived {
            peer_id,
            pending_id,
            request,
        } => {
            let code = match request.method {
                PairingMethod::Code { code } => Some(code),
                PairingMethod::Direct => None,
            };
            MobileCoreEvent::PairingRequestReceived {
                peer_id: peer_id.to_string(),
                pending_id,
                code,
            }
        }
        CoreEvent::PairingCompleted { peer_id } => MobileCoreEvent::PairingCompleted { peer_id },
        CoreEvent::TransferProgress {
            session_id,
            progress,
        } => MobileCoreEvent::TransferProgress {
            session_id: session_id.to_string(),
            progress,
        },
        CoreEvent::TransferOfferReceived { offer } => {
            MobileCoreEvent::TransferOfferReceived {
                offer: offer.into(),
            }
        }
        CoreEvent::TransferCompleted { event } => MobileCoreEvent::TransferCompleted {
            session_id: event.session_id.to_string(),
        },
        CoreEvent::TransferFailed { event } => MobileCoreEvent::TransferFailed {
            session_id: event.session_id.to_string(),
            error: event.error,
        },
        CoreEvent::TransferPaused { event } => MobileCoreEvent::TransferPaused {
            session_id: event.session_id.to_string(),
        },
        CoreEvent::TransferDbError { event } => MobileCoreEvent::TransferDbError {
            session_id: event.session_id.to_string(),
            message: event.message,
        },
        CoreEvent::Error { message } => MobileCoreEvent::Error { message },
    }
}

#[derive(uniffi::Object)]
pub struct MobileCore {
    keychain: Arc<MobileKeychainAdapter>,
    event_bus: Arc<MobileEventBusAdapter>,
    keypair: Mutex<Option<swarm_p2p_core::libp2p::identity::Keypair>>,
    net_manager: Mutex<Option<NetManager<()>>>,
    transfer_state: Mutex<MobileTransferState>,
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    #[uniffi::constructor]
    pub fn new(
        keychain: Arc<dyn ForeignKeychainProvider>,
        event_bus: Arc<dyn ForeignEventBus>,
    ) -> Arc<Self> {
        Arc::new(Self {
            keychain: Arc::new(MobileKeychainAdapter::new(keychain)),
            event_bus: Arc::new(MobileEventBusAdapter::new(event_bus)),
            keypair: Mutex::new(None),
            net_manager: Mutex::new(None),
            transfer_state: Mutex::new(MobileTransferState::default()),
        })
    }

    pub async fn initialize_identity(&self) -> Result<MobileIdentity, FfiError> {
        let identity = swarmdrop_core::identity::load_or_create_identity(self.keychain.as_ref())
            .await
            .map_err(FfiError::from)?;
        *self.keypair.lock().await = Some(identity.keypair);
        Ok(MobileIdentity {
            peer_id: identity.peer_id.to_string(),
            created: identity.created,
        })
    }

    pub async fn start_node(&self, custom_bootstrap_nodes: Vec<String>) -> Result<(), FfiError> {
        let keypair = self.ensure_keypair().await?;
        let paired_devices =
            swarmdrop_core::identity::load_paired_devices(self.keychain.as_ref()).await?;
        let started = swarmdrop_core::runtime::start_node(
            keypair,
            paired_devices,
            custom_bootstrap_nodes,
            |_| (),
        )?;

        let shared = started.manager.shared_refs();
        let client = started.manager.client().clone();
        let pairing = shared.pairing.clone();
        tokio::spawn(async move {
            let _ = pairing.announce_online().await;
            let _ = client.bootstrap().await;
            pairing.check_paired_online().await;
        });
        spawn_event_loop(started.receiver, shared, self.event_bus.clone());

        *self.net_manager.lock().await = Some(started.manager);
        Ok(())
    }

    pub async fn shutdown_node(&self) -> Result<(), FfiError> {
        let mut guard = self.net_manager.lock().await;
        if let Some(manager) = guard.as_ref() {
            manager.pairing().announce_offline().await?;
            manager.cancel_background_tasks();
        }
        guard.take();
        Ok(())
    }

    pub async fn network_status(&self) -> MobileNetworkStatus {
        self.net_manager
            .lock()
            .await
            .as_ref()
            .map(|manager| manager.get_network_status().into())
            .unwrap_or_else(|| CoreNetworkStatus::default().into())
    }

    pub async fn list_devices(&self, filter: String) -> Result<Vec<MobileDevice>, FfiError> {
        let filter = parse_device_filter(&filter)?;
        let guard = self.net_manager.lock().await;
        let manager = guard.as_ref().ok_or(FfiError::NodeNotStarted)?;
        Ok(manager
            .devices()
            .get_devices(filter)
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn transfer_sessions(&self) -> Vec<MobileTransferSession> {
        self.transfer_state
            .lock()
            .await
            .sessions
            .values()
            .cloned()
            .collect()
    }

    pub async fn transfer_session(&self, session_id: String) -> Option<MobileTransferSession> {
        self.transfer_state
            .lock()
            .await
            .sessions
            .get(&session_id)
            .cloned()
    }

    pub async fn prepare_send(
        &self,
        files: Vec<MobileTransferFile>,
    ) -> Result<MobilePreparedTransfer, FfiError> {
        if files.is_empty() {
            return Err(FfiError::Transfer("no files selected".to_string()));
        }

        let total_size = files.iter().map(|file| file.size).sum();
        let prepared = MobilePreparedTransfer {
            prepared_id: Uuid::new_v4().to_string(),
            total_size,
            files,
        };
        self.transfer_state
            .lock()
            .await
            .prepared
            .insert(prepared.prepared_id.clone(), prepared.clone());
        Ok(prepared)
    }

    pub async fn send_prepared(
        &self,
        prepared_id: String,
        peer_id: String,
        file_ids: Vec<String>,
    ) -> Result<MobileTransferSession, FfiError> {
        parse_peer_id(&peer_id)?;
        if self.net_manager.lock().await.is_none() {
            return Err(FfiError::NodeNotStarted);
        }

        let mut transfer_state = self.transfer_state.lock().await;
        let prepared = transfer_state
            .prepared
            .get(&prepared_id)
            .cloned()
            .ok_or_else(|| FfiError::Transfer("prepared transfer not found".to_string()))?;
        let selected_files: Vec<_> = if file_ids.is_empty() {
            prepared.files
        } else {
            prepared
                .files
                .into_iter()
                .filter(|file| file_ids.contains(&file.file_id))
                .collect()
        };
        if selected_files.is_empty() {
            return Err(FfiError::Transfer("no matching files selected".to_string()));
        }

        let total_size = selected_files.iter().map(|file| file.size).sum();
        let session = MobileTransferSession {
            session_id: Uuid::new_v4().to_string(),
            peer_id,
            direction: "outgoing".to_string(),
            status: "waitingForRuntime".to_string(),
            total_size,
            completed_size: 0,
            progress: 0.0,
        };
        transfer_state
            .sessions
            .insert(session.session_id.clone(), session.clone());
        Ok(session)
    }

    pub async fn accept_receive(
        &self,
        session_id: String,
        _destination_uri: String,
    ) -> Result<MobileTransferSession, FfiError> {
        if self.net_manager.lock().await.is_none() {
            return Err(FfiError::NodeNotStarted);
        }

        let mut transfer_state = self.transfer_state.lock().await;
        let session = transfer_state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| FfiError::Transfer("transfer session not found".to_string()))?;
        session.status = "accepted".to_string();
        Ok(session.clone())
    }

    pub async fn reject_receive(&self, session_id: String) -> Result<(), FfiError> {
        let mut transfer_state = self.transfer_state.lock().await;
        let session = transfer_state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| FfiError::Transfer("transfer session not found".to_string()))?;
        session.status = "rejected".to_string();
        Ok(())
    }

    pub async fn cancel_transfer(&self, session_id: String) -> Result<(), FfiError> {
        let mut transfer_state = self.transfer_state.lock().await;
        let session = transfer_state
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| FfiError::Transfer("transfer session not found".to_string()))?;
        session.status = "cancelled".to_string();
        Ok(())
    }

    pub async fn generate_pairing_code(
        &self,
        expires_in_secs: u64,
    ) -> Result<MobilePairingCode, FfiError> {
        let pairing = self.pairing_manager().await?;
        let code = pairing.generate_code(expires_in_secs).await?;
        Ok(MobilePairingCode {
            code: code.code,
            created_at: code.created_at,
            expires_at: code.expires_at,
        })
    }

    pub async fn lookup_device_by_code(
        &self,
        code: String,
    ) -> Result<MobileRemoteDeviceInfo, FfiError> {
        let pairing = self.pairing_manager().await?;
        let (peer_id, record) = pairing.get_device_info(&code).await?;
        Ok(MobileRemoteDeviceInfo::from_record(peer_id, record))
    }

    pub async fn request_pairing(
        &self,
        peer_id: String,
        code: Option<String>,
        addrs: Vec<String>,
    ) -> Result<MobilePairingResult, FfiError> {
        let pairing = self.pairing_manager().await?;
        let peer_id = parse_peer_id(&peer_id)?;
        let addrs = parse_multiaddrs(addrs)?;
        let method = code
            .map(|code| PairingMethod::Code { code })
            .unwrap_or(PairingMethod::Direct);
        let (response, paired) = pairing
            .request_pairing(peer_id, method, Some(addrs))
            .await?;
        if let Some(info) = paired {
            swarmdrop_core::identity::upsert_paired_device(self.keychain.as_ref(), info).await?;
        }
        Ok(pairing_result(response))
    }

    pub async fn respond_pairing_request(
        &self,
        pending_id: u64,
        code: Option<String>,
        accept: bool,
    ) -> Result<(), FfiError> {
        let pairing = self.pairing_manager().await?;
        let method = code
            .map(|code| PairingMethod::Code { code })
            .unwrap_or(PairingMethod::Direct);
        let response = if accept {
            PairingResponse::Success
        } else {
            PairingResponse::Refused {
                reason: PairingRefuseReason::UserRejected,
            }
        };

        if let Some(info) = pairing
            .handle_pairing_request(pending_id, &method, response)
            .await?
        {
            swarmdrop_core::identity::upsert_paired_device(self.keychain.as_ref(), info).await?;
        }

        Ok(())
    }
}

impl MobileCore {
    async fn pairing_manager(
        &self,
    ) -> Result<Arc<swarmdrop_core::pairing::manager::PairingManager>, FfiError> {
        self.net_manager
            .lock()
            .await
            .as_ref()
            .map(|manager| manager.shared_refs().pairing)
            .ok_or(FfiError::NodeNotStarted)
    }

    async fn ensure_keypair(
        &self,
    ) -> Result<swarm_p2p_core::libp2p::identity::Keypair, FfiError> {
        if let Some(keypair) = self.keypair.lock().await.as_ref().cloned() {
            return Ok(keypair);
        }
        let identity = swarmdrop_core::identity::load_or_create_identity(self.keychain.as_ref())
            .await
            .map_err(FfiError::from)?;
        let keypair = identity.keypair;
        *self.keypair.lock().await = Some(keypair.clone());
        Ok(keypair)
    }
}

fn parse_peer_id(value: &str) -> Result<PeerId, FfiError> {
    value
        .parse()
        .map_err(|error| FfiError::Identity(format!("invalid peer id: {error}")))
}

fn parse_multiaddrs(values: Vec<String>) -> Result<Vec<Multiaddr>, FfiError> {
    values
        .into_iter()
        .map(|value| {
            value
                .parse()
                .map_err(|error| FfiError::Network(format!("invalid multiaddr: {error}")))
        })
        .collect()
}

fn pairing_result(response: PairingResponse) -> MobilePairingResult {
    match response {
        PairingResponse::Success => MobilePairingResult {
            accepted: true,
            reason: None,
        },
        PairingResponse::Refused { reason } => MobilePairingResult {
            accepted: false,
            reason: Some(format!("{reason:?}")),
        },
    }
}

fn parse_device_filter(value: &str) -> Result<DeviceFilter, FfiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "all" => Ok(DeviceFilter::All),
        "connected" => Ok(DeviceFilter::Connected),
        "paired" => Ok(DeviceFilter::Paired),
        other => Err(FfiError::Identity(format!("invalid device filter: {other}"))),
    }
}

fn spawn_event_loop(
    mut receiver: EventReceiver<AppRequest>,
    shared: swarmdrop_core::network::SharedNetRefs<()>,
    event_bus: Arc<MobileEventBusAdapter>,
) {
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if let Err(error) =
                swarmdrop_core::network::event_loop::handle_core_node_event(
                    &shared,
                    &event,
                    event_bus.as_ref(),
                )
                .await
            {
                let _ = event_bus
                    .publish(CoreEvent::Error {
                        message: error.to_string(),
                    })
                    .await;
            }
        }
    });
}
