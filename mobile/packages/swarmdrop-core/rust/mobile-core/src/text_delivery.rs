//! 文本投递的 UniFFI 边界。
//!
//! 正文与重试语义留在 core；移动端只拿到稳定的 Record/Enum 镜像，不在 TypeScript 侧重建
//! 状态机，也不通过剪贴板读取来隐式发送。
use swarmdrop_core::transfer::text_delivery::{PendingTextDeliverySummary, TextDeliveryRecord};
use uuid::Uuid;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::utils::parse_peer_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTextDeliveryDirection {
    Send,
    Receive,
}

impl From<entity::TextDeliveryDirection> for MobileTextDeliveryDirection {
    fn from(value: entity::TextDeliveryDirection) -> Self {
        match value {
            entity::TextDeliveryDirection::Send => Self::Send,
            entity::TextDeliveryDirection::Receive => Self::Receive,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTextDeliveryStatus {
    Sending,
    WaitingConfirmation,
    Delivered,
    Rejected,
    Retryable,
    Expired,
    Cancelled,
}

impl From<entity::TextDeliveryStatus> for MobileTextDeliveryStatus {
    fn from(value: entity::TextDeliveryStatus) -> Self {
        match value {
            entity::TextDeliveryStatus::Sending => Self::Sending,
            entity::TextDeliveryStatus::WaitingConfirmation => Self::WaitingConfirmation,
            entity::TextDeliveryStatus::Delivered => Self::Delivered,
            entity::TextDeliveryStatus::Rejected => Self::Rejected,
            entity::TextDeliveryStatus::Retryable => Self::Retryable,
            entity::TextDeliveryStatus::Expired => Self::Expired,
            entity::TextDeliveryStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileTextDeliveryFailure {
    PeerUnavailable,
    TimedOut,
    UnsupportedProtocol,
    Rejected,
    Expired,
    StorageFailed,
    ProtocolConflict,
    InvalidPayload,
}

impl From<entity::TextDeliveryFailure> for MobileTextDeliveryFailure {
    fn from(value: entity::TextDeliveryFailure) -> Self {
        match value {
            entity::TextDeliveryFailure::PeerUnavailable => Self::PeerUnavailable,
            entity::TextDeliveryFailure::TimedOut => Self::TimedOut,
            entity::TextDeliveryFailure::UnsupportedProtocol => Self::UnsupportedProtocol,
            entity::TextDeliveryFailure::Rejected => Self::Rejected,
            entity::TextDeliveryFailure::Expired => Self::Expired,
            entity::TextDeliveryFailure::StorageFailed => Self::StorageFailed,
            entity::TextDeliveryFailure::ProtocolConflict => Self::ProtocolConflict,
            entity::TextDeliveryFailure::InvalidPayload => Self::InvalidPayload,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileTextDeliveryRecord {
    pub delivery_id: String,
    pub direction: MobileTextDeliveryDirection,
    pub peer_id: String,
    pub peer_name: String,
    pub body: String,
    pub status: MobileTextDeliveryStatus,
    pub failure: Option<MobileTextDeliveryFailure>,
    pub attempt_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<TextDeliveryRecord> for MobileTextDeliveryRecord {
    fn from(record: TextDeliveryRecord) -> Self {
        Self {
            delivery_id: record.delivery_id.to_string(),
            direction: record.direction.into(),
            peer_id: record.peer_id,
            peer_name: record.peer_name,
            body: record.body,
            status: record.status.into(),
            failure: record.failure.map(Into::into),
            attempt_count: record.attempt_count.max(0) as u32,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobilePendingTextDelivery {
    pub delivery_id: String,
    pub peer_id: String,
    pub peer_name: String,
    pub body: String,
    pub created_at: i64,
}

impl From<PendingTextDeliverySummary> for MobilePendingTextDelivery {
    fn from(value: PendingTextDeliverySummary) -> Self {
        Self {
            delivery_id: value.delivery_id.to_string(),
            peer_id: value.peer_id,
            peer_name: value.peer_name,
            body: value.body,
            created_at: value.created_at,
        }
    }
}

fn parse_delivery_id(value: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|_| FfiError::Transfer(format!("invalid text delivery id: {value}")))
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn send_text_delivery(
        &self,
        peer_id: String,
        peer_name: String,
        body: String,
    ) -> FfiResult<MobileTextDeliveryRecord> {
        parse_peer_id(&peer_id)?;
        let manager = self.transfer_manager_arc().await?;
        manager
            .send_text_delivery(peer_id, peer_name, body)
            .await
            .map(Into::into)
            .map_err(FfiError::from)
    }

    pub async fn retry_text_delivery(
        &self,
        delivery_id: String,
    ) -> FfiResult<MobileTextDeliveryRecord> {
        let manager = self.transfer_manager_arc().await?;
        manager
            .retry_text_delivery(parse_delivery_id(&delivery_id)?)
            .await
            .map(Into::into)
            .map_err(FfiError::from)
    }

    pub async fn list_text_outbox(
        &self,
        peer_id: String,
    ) -> FfiResult<Vec<MobileTextDeliveryRecord>> {
        let store = self.ensure_store().await?;
        store
            .list_outgoing_text_deliveries(&peer_id)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(FfiError::from)
    }

    pub async fn delete_text_outbox_record(&self, delivery_id: String) -> FfiResult<()> {
        let store = self.ensure_store().await?;
        store
            .delete_outgoing_text_delivery(parse_delivery_id(&delivery_id)?)
            .await
            .map_err(FfiError::from)
    }

    pub async fn pending_text_deliveries(&self) -> FfiResult<Vec<MobilePendingTextDelivery>> {
        let manager = self.transfer_manager_arc().await?;
        Ok(manager
            .text_delivery_service()?
            .pending()
            .await
            .map_err(FfiError::from)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn confirm_text_delivery(
        &self,
        delivery_id: String,
        accepted: bool,
    ) -> FfiResult<()> {
        let manager = self.transfer_manager_arc().await?;
        let service = manager.text_delivery_service()?;
        if accepted {
            service.accept(parse_delivery_id(&delivery_id)?).await?;
        } else {
            service.reject(parse_delivery_id(&delivery_id)?).await?;
        }
        Ok(())
    }
}
