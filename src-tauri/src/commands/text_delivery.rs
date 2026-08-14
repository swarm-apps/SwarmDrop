//! 文本投递 IPC 薄壳。

use swarmdrop_core::transfer::text_delivery::TextDeliveryRecord;
use swarmdrop_core::transfer::text_service::PendingTextDeliverySummary;
use tauri::State;
use uuid::Uuid;

use crate::database::TransferStoreState;
use crate::network::NetManagerState;

#[tauri::command]
#[specta::specta]
pub async fn send_text_delivery(
    net: State<'_, NetManagerState>,
    peer_id: String,
    peer_name: String,
    body: String,
) -> crate::AppResult<TextDeliveryRecord> {
    let transfer = super::transfer::get_transfer(&net).await?;
    Ok(transfer
        .send_text_delivery(peer_id, peer_name, body)
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn retry_text_delivery(
    net: State<'_, NetManagerState>,
    delivery_id: Uuid,
) -> crate::AppResult<TextDeliveryRecord> {
    let transfer = super::transfer::get_transfer(&net).await?;
    Ok(transfer.retry_text_delivery(delivery_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn confirm_text_delivery(
    net: State<'_, NetManagerState>,
    delivery_id: Uuid,
    accepted: bool,
) -> crate::AppResult<()> {
    let transfer = super::transfer::get_transfer(&net).await?;
    let service = transfer.text_delivery_service()?;
    if accepted {
        service.accept(delivery_id).await?
    } else {
        service.reject(delivery_id)?
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn pending_text_deliveries(
    net: State<'_, NetManagerState>,
) -> crate::AppResult<Vec<PendingTextDeliverySummary>> {
    let transfer = super::transfer::get_transfer(&net).await?;
    Ok(transfer.text_delivery_service()?.pending())
}

#[tauri::command]
#[specta::specta]
pub async fn list_text_outbox(
    store: State<'_, TransferStoreState>,
    peer_id: String,
) -> crate::AppResult<Vec<TextDeliveryRecord>> {
    Ok(store.list_outgoing_text_deliveries(&peer_id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_text_outbox_record(
    store: State<'_, TransferStoreState>,
    delivery_id: Uuid,
) -> crate::AppResult<()> {
    Ok(store.delete_outgoing_text_delivery(delivery_id).await?)
}
