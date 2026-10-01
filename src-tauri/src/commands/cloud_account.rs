use crate::error::AppResult;
use std::sync::Arc;
use swarmdrop_cloud_auth::{
    AccountSnapshot, ClientCredentials, CloudAccountManager, ConnectSession, ProviderId,
    RevokeOutcome,
};
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn list_cloud_accounts(
    manager: State<'_, Arc<CloudAccountManager>>,
) -> AppResult<Vec<AccountSnapshot>> {
    Ok(manager.list().await)
}

#[tauri::command]
#[specta::specta]
pub async fn connect_cloud_account(
    manager: State<'_, Arc<CloudAccountManager>>,
    client_id: String,
    client_secret: String,
    label: String,
) -> AppResult<ConnectSession> {
    Ok(manager
        .inner()
        .start_connect(
            ProviderId::GoogleDrive,
            ClientCredentials {
                client_id,
                client_secret,
            },
            label,
            None,
        )
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn reconnect_cloud_account(
    manager: State<'_, Arc<CloudAccountManager>>,
    id: String,
) -> AppResult<ConnectSession> {
    Ok(manager.inner().reconnect(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_cloud_account_connect(
    manager: State<'_, Arc<CloudAccountManager>>,
    session_id: String,
) -> AppResult<()> {
    manager.cancel_connect(&session_id).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn disconnect_cloud_account(
    manager: State<'_, Arc<CloudAccountManager>>,
    id: String,
) -> AppResult<RevokeOutcome> {
    Ok(manager.disconnect(&id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_account_status(
    manager: State<'_, Arc<CloudAccountManager>>,
    id: String,
) -> AppResult<AccountSnapshot> {
    Ok(manager.status(&id).await?)
}
