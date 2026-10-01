//! 可进入用户界面的账户视图；不包含凭证。
use crate::{CloudAuthError, ProviderId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum AccountStatus {
    Connected,
    Refreshing,
    ReconnectRequired,
    SaveFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AccountSnapshot {
    pub id: String,
    pub provider: ProviderId,
    pub label: String,
    pub status: AccountStatus,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AccountUpdate {
    pub session_id: Option<String>,
    pub account: Option<AccountSnapshot>,
    pub removed_account_id: Option<String>,
    pub error: Option<CloudAuthError>,
}
