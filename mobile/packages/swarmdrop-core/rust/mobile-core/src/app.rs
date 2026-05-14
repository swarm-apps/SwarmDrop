//! `MobileCore` —— RN 持有的主对象。
//!
//! - constructor 是 sync(Rust `pub fn new`),RN 侧直接 `new MobileCore(keychain, eventBus)`
//! - 其他业务方法分散在 identity / network / device / pairing / transfer 各模块,
//!   通过 `impl MobileCore` 跨模块挂接(Rust 允许多个 impl 块,ubrn proc-macro 都能扫到)
//! - 私有 fields 用 `pub(crate)` 访问器暴露给同 crate 内的业务模块,不暴露给外部

use std::sync::Arc;

use swarm_p2p_core::libp2p::identity::Keypair;
use swarmdrop_core::host::KeychainProvider;
use swarmdrop_core::network::NetManager;
use swarmdrop_core::pairing::manager::PairingManager;
use tokio::sync::{Mutex, MutexGuard};

use crate::error::{FfiError, FfiResult};
use crate::events::{ForeignEventBus, MobileEventBusAdapter};
use crate::keychain::{ForeignKeychainProvider, MobileKeychainAdapter};
use crate::transfer::MobileTransferState;

#[derive(uniffi::Object)]
pub struct MobileCore {
    keychain: Arc<MobileKeychainAdapter>,
    event_bus: Arc<MobileEventBusAdapter>,
    keypair: Mutex<Option<Keypair>>,
    net_manager: Mutex<Option<NetManager<()>>>,
    transfer_state: Mutex<MobileTransferState>,
}

#[uniffi::export]
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
}

// ── 私有 helpers ─────────────────────────────────────────────
//
// `pub(crate)` 访问器给同 crate 内业务模块用。不加 #[uniffi::export],
// 不会出现在 TS bindings 里。

impl MobileCore {
    pub(crate) fn keychain(&self) -> &dyn KeychainProvider {
        self.keychain.as_ref()
    }

    pub(crate) fn event_bus_arc(&self) -> Arc<MobileEventBusAdapter> {
        self.event_bus.clone()
    }

    pub(crate) async fn set_keypair(&self, keypair: Keypair) {
        *self.keypair.lock().await = Some(keypair);
    }

    pub(crate) async fn ensure_keypair(&self) -> FfiResult<Keypair> {
        if let Some(keypair) = self.keypair.lock().await.as_ref().cloned() {
            return Ok(keypair);
        }
        let identity = swarmdrop_core::identity::load_or_create_identity(self.keychain())
            .await
            .map_err(FfiError::from)?;
        let keypair = identity.keypair;
        *self.keypair.lock().await = Some(keypair.clone());
        Ok(keypair)
    }

    pub(crate) async fn net_manager_guard(&self) -> MutexGuard<'_, Option<NetManager<()>>> {
        self.net_manager.lock().await
    }

    pub(crate) async fn set_net_manager(&self, manager: NetManager<()>) {
        *self.net_manager.lock().await = Some(manager);
    }

    pub(crate) async fn pairing_manager(&self) -> FfiResult<Arc<PairingManager>> {
        self.net_manager
            .lock()
            .await
            .as_ref()
            .map(|manager| manager.shared_refs().pairing)
            .ok_or(FfiError::NodeNotStarted)
    }

    pub(crate) async fn transfer_state(&self) -> MutexGuard<'_, MobileTransferState> {
        self.transfer_state.lock().await
    }

    pub(crate) async fn transfer_state_mut(&self) -> MutexGuard<'_, MobileTransferState> {
        self.transfer_state.lock().await
    }
}
