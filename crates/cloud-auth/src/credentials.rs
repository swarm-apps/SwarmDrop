//! 进程内凭证与访问租约，格式化时不得暴露秘密。
use crate::{AccountSnapshot, CloudAuthError};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// 不派生 Debug：授权配置与 token 的任何格式化都只能走显式脱敏视图。
#[derive(Clone, Serialize, Deserialize)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
}
impl ClientCredentials {
    pub fn validate(&self) -> Result<(), CloudAuthError> {
        if self.client_id.trim().is_empty() || self.client_secret.trim().is_empty() {
            Err(CloudAuthError::InvalidConfiguration)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TokenMaterial {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AccountCredentials {
    pub snapshot: AccountSnapshot,
    pub client: ClientCredentials,
    pub token: TokenMaterial,
}

#[derive(Clone)]
pub struct AccessTokenLease {
    token: String,
    pub expires_at: u64,
}
impl AccessTokenLease {
    pub(crate) fn new(token: &TokenMaterial) -> Self {
        Self {
            token: token.access_token.clone(),
            expires_at: token.expires_at,
        }
    }
    pub fn secret(&self) -> &str {
        &self.token
    }
}
impl fmt::Debug for AccessTokenLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessTokenLease")
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}
