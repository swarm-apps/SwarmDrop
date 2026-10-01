//! 授权提供方负责交互与 token 协议，账户管理器负责生命周期。
use crate::{ClientCredentials, CloudAuthError, TokenMaterial};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    GoogleDrive,
}

impl ProviderId {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::GoogleDrive => "Google Drive",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GoogleDrive => "google-drive",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ConnectSession {
    pub id: String,
    pub authorization_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RevokeOutcome {
    pub revoked: bool,
}

pub struct PendingConnect {
    pub session: ConnectSession,
    pub completion: Pin<Box<dyn Future<Output = Result<TokenMaterial, CloudAuthError>> + Send>>,
}

#[async_trait]
pub trait CloudAccountProvider: Send + Sync {
    fn provider_id(&self) -> ProviderId;
    async fn start_connect(
        &self,
        client: ClientCredentials,
    ) -> Result<PendingConnect, CloudAuthError>;
    async fn refresh(
        &self,
        client: &ClientCredentials,
        token: &TokenMaterial,
    ) -> Result<TokenMaterial, CloudAuthError>;
    async fn revoke(&self, token: &TokenMaterial) -> RevokeOutcome;
}
