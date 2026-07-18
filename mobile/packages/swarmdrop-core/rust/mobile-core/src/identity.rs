//! 身份 —— Ed25519 keypair 装载/创建,首次启动会写入 keychain。

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileIdentity {
    pub peer_id: String,
    pub created: bool,
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn initialize_identity(&self) -> FfiResult<MobileIdentity> {
        let identity = swarmdrop_core::identity::load_or_create_identity(self.keychain())
            .await
            .map_err(FfiError::from)?;
        self.set_keypair(identity.secret_key.clone()).await;
        Ok(MobileIdentity {
            peer_id: identity.node_id.to_string(),
            created: identity.created,
        })
    }
}
