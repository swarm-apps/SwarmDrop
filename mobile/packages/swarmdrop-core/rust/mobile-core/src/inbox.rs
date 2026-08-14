//! Drop Inbox bridge.
//!
//! Inbox is the received-content ledger. It is intentionally separate from
//! transfer activity projections so clearing Activity never removes received
//! content records.
//!
//! 查询与标记一律经 `InboxStore` 端口（`MobileCore::ensure_store()` 取回的那一份），
//! 不再直连 SeaORM 自由函数 —— 与 `history.rs` 走传输端口是同一条纪律。
//! DTO 也随之来自 `swarmdrop_core::transfer::inbox`（领域侧），不再来自某个存储实现。

use entity::{InboxContentKind, InboxSourceKind};
use swarmdrop_core::transfer::inbox::{
    InboxHitFile, InboxItemContent, InboxItemDetail, InboxItemFileEntry, InboxItemSummary,
    InboxSearchHit,
};
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
    pub text_delivery_id: Option<String>,
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

impl From<InboxItemSummary> for MobileInboxItemSummary {
    fn from(item: InboxItemSummary) -> Self {
        // 穷尽解构：上游 InboxItemSummary 新增字段时此处会编译失败（drift guard）。
        let InboxItemSummary {
            id,
            transfer_session_id,
            text_delivery_id,
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
            text_delivery_id: text_delivery_id.map(|id| id.to_string()),
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

impl From<InboxItemFileEntry> for MobileInboxFileEntry {
    fn from(file: InboxItemFileEntry) -> Self {
        // 穷尽解构：上游 InboxItemFileEntry 新增字段时此处会编译失败（drift guard）。
        let InboxItemFileEntry {
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

#[derive(Debug, Clone, uniffi::Enum)]
pub enum MobileInboxItemContent {
    Files {
        entries: Vec<MobileInboxFileEntry>,
        transfer: Option<MobileTransferProjection>,
    },
    Text {
        body: String,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInboxItemDetail {
    pub item: MobileInboxItemSummary,
    pub content: MobileInboxItemContent,
}

impl From<InboxItemDetail> for MobileInboxItemDetail {
    fn from(detail: InboxItemDetail) -> Self {
        // 穷尽解构：上游 InboxItemDetail 新增字段时此处会编译失败（drift guard）。
        let InboxItemDetail { item, content } = detail;
        Self {
            item: item.into(),
            content: match content {
                InboxItemContent::Files { entries, transfer } => MobileInboxItemContent::Files {
                    entries: entries.into_iter().map(Into::into).collect(),
                    transfer: transfer.map(Into::into),
                },
                InboxItemContent::Text { body } => MobileInboxItemContent::Text { body },
            },
        }
    }
}

/// 收件箱全文检索命中条目下的单个文件（文件名 + 相对路径），供下钻展示。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInboxHitFile {
    pub name: String,
    pub relative_path: String,
}

impl From<InboxHitFile> for MobileInboxHitFile {
    fn from(file: InboxHitFile) -> Self {
        // 穷尽解构（drift guard）：上游 InboxHitFile 新增字段时此处会编译失败。
        let InboxHitFile {
            name,
            relative_path,
        } = file;
        Self {
            name,
            relative_path,
        }
    }
}

/// 收件箱全文检索（FTS）命中项，镜像 core `InboxSearchHit`。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileInboxSearchHit {
    pub id: String,
    pub title: String,
    pub source_name: String,
    pub item_count: u32,
    pub root_path: Option<String>,
    pub received_at: i64,
    /// 命中所在文本的片段（core 端按子串位置切窗口生成）。
    ///
    /// `None` = 不该渲染片段行：命中的是标题或来源名（条目行上已经显示着），或一个候选都
    /// 没命中。判据在 core 的 `inbox_snippet`，端上不要再判一遍。
    pub snippet: Option<String>,
    pub files: Vec<MobileInboxHitFile>,
}

impl From<InboxSearchHit> for MobileInboxSearchHit {
    fn from(hit: InboxSearchHit) -> Self {
        // 穷尽解构（drift guard）：上游 InboxSearchHit 新增字段时此处会编译失败。
        let InboxSearchHit {
            id,
            title,
            source_name,
            item_count,
            root_path,
            received_at,
            snippet,
            files,
        } = hit;
        Self {
            id: id.to_string(),
            title,
            source_name,
            item_count: item_count.max(0) as u32,
            root_path,
            received_at,
            snippet,
            files: files.into_iter().map(Into::into).collect(),
        }
    }
}

fn parse_item_id(s: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| FfiError::Transfer(format!("invalid inbox item id: {s}")))
}

fn parse_session_id(s: &str) -> FfiResult<Uuid> {
    Uuid::parse_str(s).map_err(|_| FfiError::Transfer(format!("invalid session_id: {s}")))
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
        let store = self.ensure_store().await?;
        let items = store
            .list_inbox_items(include_archived)
            .await
            .map_err(FfiError::from)?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn get_inbox_item(
        &self,
        item_id: String,
    ) -> FfiResult<Option<MobileInboxItemDetail>> {
        let item_uuid = parse_item_id(&item_id)?;
        let store = self.ensure_store().await?;
        let item = store
            .get_inbox_item_detail(item_uuid)
            .await
            .map_err(FfiError::from)?;
        Ok(item.map(Into::into))
    }

    pub async fn get_inbox_item_by_transfer_session_id(
        &self,
        session_id: String,
    ) -> FfiResult<Option<MobileInboxItemDetail>> {
        let session_uuid = parse_session_id(&session_id)?;
        let store = self.ensure_store().await?;
        let item = store
            .get_inbox_item_by_transfer_session_id(session_uuid)
            .await
            .map_err(FfiError::from)?;
        Ok(item.map(Into::into))
    }

    pub async fn mark_inbox_item_opened(&self, item_id: String) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let store = self.ensure_store().await?;
        store
            .mark_inbox_item_opened(item_uuid)
            .await
            .map_err(FfiError::from)
    }

    pub async fn archive_inbox_item(&self, item_id: String, archived: bool) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let store = self.ensure_store().await?;
        store
            .archive_inbox_item(item_uuid, archived)
            .await
            .map_err(FfiError::from)
    }

    /// **只删账本**，不碰文件。要连文件一起删走 [`Self::delete_inbox_item`]。
    pub async fn delete_inbox_item_record(&self, item_id: String) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let store = self.ensure_store().await?;
        store
            .delete_inbox_item_record(item_uuid)
            .await
            .map_err(FfiError::from)
    }

    /// 删除收件箱条目；`delete_local_files` 为真时连已落盘的文件一起删。
    ///
    /// 编排（先文件后记录、删文件失败不阻断、条目不存在报错）住在
    /// [`swarmdrop_core::transfer::inbox::delete_inbox_item`]，三端共用。此前**这段编排在
    /// TS 里**（`inbox-store.ts` 的 `deleteLocalFiles` + 手写的顺序），于是同一段逻辑
    /// 三端各一份，且那份在 detail 取不到时静默跳过、另两端报错。
    ///
    /// 「`file://` / SAF URI 怎么删」那一层仍在 JS（`ForeignFileAccess::delete_finalized_file`）
    /// ——那才是真正的平台细节。
    pub async fn delete_inbox_item(
        &self,
        item_id: String,
        delete_local_files: bool,
    ) -> FfiResult<()> {
        let item_uuid = parse_item_id(&item_id)?;
        let store = self.ensure_store().await?;
        let file_access = self.file_access_arc();
        swarmdrop_core::transfer::inbox::delete_inbox_item(
            store.as_ref(),
            file_access.as_ref(),
            item_uuid,
            delete_local_files,
        )
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
        // 「file_id 必须属于 item_id」的归属校验是端口的义务（见 `InboxStore` 文档），
        // 桥接层只负责把 item_id 传下去 —— 这条检查此前是移动端独有的，现已三端同规。
        let store = self.ensure_store().await?;
        store
            .mark_inbox_item_file_missing(item_uuid, file_id_i32, missing)
            .await
            .map_err(FfiError::from)
    }

    pub async fn repair_missing_inbox_items(&self) -> FfiResult<Vec<MobileInboxItemDetail>> {
        let store = self.ensure_store().await?;
        let repaired = store
            .repair_missing_inbox_items_for_completed_receives()
            .await
            .map_err(FfiError::from)?;
        Ok(repaired.into_iter().map(Into::into).collect())
    }

    /// 收件箱全文检索（FTS）：匹配标题 / 来源 / 文件名+相对路径 / 文档正文，
    /// 按 received_at 倒序返回带 snippet 的命中项。镜像桌面 `search_inbox`（core 3d2d764）。
    /// `limit` 缺省取三端共享的 [`INBOX_SEARCH_LIMIT`]——端上不要自带这个数字。
    /// 此前 JS 侧写死 100、桌面 20、Web 50，而截断掉的永远是最早收到的那批，
    /// 于是同一个查询词在两端搜出不同结果（#111）。
    pub async fn search_inbox(
        &self,
        query: String,
        limit: Option<u32>,
        include_archived: bool,
    ) -> FfiResult<Vec<MobileInboxSearchHit>> {
        let store = self.ensure_store().await?;
        let hits = store
            .search_inbox_capped(&query, limit, include_archived)
            .await
            .map_err(FfiError::from)?;
        Ok(hits.into_iter().map(Into::into).collect())
    }
}
