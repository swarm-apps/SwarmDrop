//! Drop Inbox bridge.
//!
//! Inbox is the received-content ledger. It is intentionally separate from
//! transfer activity projections so clearing Activity never removes received
//! content records.

use entity::{InboxContentKind, InboxSourceKind};
use swarmdrop_core::database::inbox as inbox_ops;
use uuid::Uuid;

use crate::app::MobileCore;
use crate::error::{FfiError, FfiResult};
use crate::history::MobileTransferProjection;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileInboxSourceKind {
    PairedDevice,
    ShareCode,
    Mcp,
    Unknown,
}

impl From<InboxSourceKind> for MobileInboxSourceKind {
    fn from(kind: InboxSourceKind) -> Self {
        match kind {
            InboxSourceKind::PairedDevice => Self::PairedDevice,
            InboxSourceKind::ShareCode => Self::ShareCode,
            InboxSourceKind::Mcp => Self::Mcp,
            InboxSourceKind::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileInboxContentKind {
    Files,
    Text,
    Clipboard,
    Bundle,
}

impl From<InboxContentKind> for MobileInboxContentKind {
    fn from(kind: InboxContentKind) -> Self {
        match kind {
            InboxContentKind::Files => Self::Files,
            InboxContentKind::Text => Self::Text,
            InboxContentKind::Clipboard => Self::Clipboard,
            InboxContentKind::Bundle => Self::Bundle,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInboxItemSummary {
    pub id: String,
    pub transfer_session_id: Option<String>,
    pub source_peer_id: String,
    pub source_name: String,
    pub source_kind: MobileInboxSourceKind,
    pub content_kind: MobileInboxContentKind,
    pub title: String,
    pub item_count: u32,
    pub total_size: u64,
    pub root_path: Option<String>,
    pub content_hash: Option<String>,
    pub received_at: i64,
    pub last_opened_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub missing: bool,
}

impl From<inbox_ops::InboxItemSummary> for MobileInboxItemSummary {
    fn from(item: inbox_ops::InboxItemSummary) -> Self {
        // 穷尽解构：上游 InboxItemSummary 新增字段时此处会编译失败（drift guard）。
        let inbox_ops::InboxItemSummary {
            id,
            transfer_session_id,
            source_peer_id,
            source_name,
            source_kind,
            content_kind,
            title,
            item_count,
            total_size,
            root_path,
            content_hash,
            received_at,
            last_opened_at,
            archived_at,
            deleted_at,
            missing,
        } = item;
        Self {
            id: id.to_string(),
            transfer_session_id: transfer_session_id.map(|id| id.to_string()),
            source_peer_id,
            source_name,
            source_kind: source_kind.into(),
            content_kind: content_kind.into(),
            title,
            item_count: item_count.max(0) as u32,
            total_size: total_size.max(0) as u64,
            root_path,
            content_hash,
            received_at,
            last_opened_at,
            archived_at,
            deleted_at,
            missing,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInboxFileEntry {
    pub id: u32,
    pub transfer_file_id: Option<u32>,
    pub relative_path: String,
    pub name: String,
    pub size: u64,
    pub checksum: String,
    pub local_path: String,
    pub missing: bool,
}

impl From<inbox_ops::InboxItemFileEntry> for MobileInboxFileEntry {
    fn from(file: inbox_ops::InboxItemFileEntry) -> Self {
        // 穷尽解构：上游 InboxItemFileEntry 新增字段时此处会编译失败（drift guard）。
        let inbox_ops::InboxItemFileEntry {
            id,
            transfer_file_id,
            relative_path,
            name,
            size,
            checksum,
            local_path,
            missing,
        } = file;
        Self {
            id: id.max(0) as u32,
            transfer_file_id: transfer_file_id.map(|id| id.max(0) as u32),
            relative_path,
            name,
            size: size.max(0) as u64,
            checksum,
            local_path,
            missing,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInboxItemDetail {
    pub item: MobileInboxItemSummary,
    pub files: Vec<MobileInboxFileEntry>,
    pub transfer: Option<MobileTransferProjection>,
}

impl From<inbox_ops::InboxItemDetail> for MobileInboxItemDetail {
    fn from(detail: inbox_ops::InboxItemDetail) -> Self {
        // 穷尽解构：上游 InboxItemDetail 新增字段时此处会编译失败（drift guard）。
        let inbox_ops::InboxItemDetail {
            item,
            files,
            transfer,
        } = detail;
        Self {
            item: item.into(),
            files: files.into_iter().map(Into::into).collect(),
            transfer: transfer.map(Into::into),
        }
    }
}

fn parse_item_id(s: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| FfiError::Transfer(format!("invalid inbox item id: {s}")))
}

fn parse_file_id(file_id: u32) -> FfiResult<i32> {
    i32::try_from(file_id)
        .map_err(|_| FfiError::Transfer(format!("invalid inbox file id: {file_id}")))
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileCore {
    pub async fn list_inbox_items(
        &self,
        include_archived: bool,
    ) -> FfiResult<Vec<MobileInboxItemSummary>> {
        let db = self.ensure_db().await?;
        let items = inbox_ops::list_inbox_items(&db, include_archived)
            .await
            .map_err(FfiError::from)?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn get_inbox_item(
        &self,
        item_id: String,
    ) -> FfiResult<Option<MobileInboxItemDetail>> {
        let item_uuid = parse_item_id(&item_id)?;
        let db = self.ensure_db().await?;
        let item = inbox_ops::get_inbox_item_detail(&db, item_uuid)
            .await
            .map_err(FfiError::from)?;
        Ok(item.map(Into::into))
    }

    pub async fn mark_inbox_item_opened(&self, item_id: String) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let db = self.ensure_db().await?;
        inbox_ops::mark_inbox_item_opened(&db, item_uuid)
            .await
            .map_err(FfiError::from)
    }

    pub async fn archive_inbox_item(&self, item_id: String, archived: bool) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let db = self.ensure_db().await?;
        inbox_ops::archive_inbox_item(&db, item_uuid, archived)
            .await
            .map_err(FfiError::from)
    }

    pub async fn delete_inbox_item_record(&self, item_id: String) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let db = self.ensure_db().await?;
        inbox_ops::delete_inbox_item_record(&db, item_uuid)
            .await
            .map_err(FfiError::from)
    }

    pub async fn mark_inbox_file_missing(
        &self,
        item_id: String,
        file_id: u32,
        missing: bool,
    ) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let file_id_i32 = parse_file_id(file_id)?;
        let db = self.ensure_db().await?;
        let detail = inbox_ops::get_inbox_item_detail(&db, item_uuid)
            .await
            .map_err(FfiError::from)?
            .ok_or_else(|| FfiError::Transfer("inbox item not found".into()))?;
        if !detail.files.iter().any(|file| file.id == file_id_i32) {
            return Err(FfiError::Transfer(
                "inbox file does not belong to item".into(),
            ));
        }
        inbox_ops::mark_inbox_item_file_missing(&db, file_id_i32, missing)
            .await
            .map_err(FfiError::from)
    }

    pub async fn repair_missing_inbox_items(&self) -> FfiResult<Vec<MobileInboxItemDetail>> {
        let db = self.ensure_db().await?;
        let repaired = inbox_ops::repair_missing_inbox_items_for_completed_receives(&db)
            .await
            .map_err(FfiError::from)?;
        Ok(repaired.into_iter().map(Into::into).collect())
    }
}
