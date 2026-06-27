//! Drop Inbox 数据访问。
//!
//! 收件箱是“已接收内容索引”，与 transfer_sessions / transfer_files 的过程账本分开维护。

use entity::{InboxContentKind, InboxSourceKind, TerminalReason, TransferDirection, TransferPhase};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityLoaderTrait, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::AppResult;
use crate::database::ops::{TransferProjection, get_transfer_projection, now_ms};
use crate::host::CoreSaveLocation;

/// 收件箱列表条目 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxItemSummary {
    pub id: Uuid,
    pub transfer_session_id: Option<Uuid>,
    pub source_peer_id: String,
    pub source_name: String,
    pub source_kind: InboxSourceKind,
    pub content_kind: InboxContentKind,
    pub title: String,
    pub item_count: i32,
    pub total_size: i64,
    pub root_path: Option<String>,
    pub content_hash: Option<String>,
    pub received_at: i64,
    pub last_opened_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub missing: bool,
}

/// 收件箱文件 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxItemFileEntry {
    pub id: i32,
    pub transfer_file_id: Option<i32>,
    pub relative_path: String,
    pub name: String,
    pub size: i64,
    pub checksum: String,
    pub local_path: String,
    pub missing: bool,
}

/// 收件箱详情 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxItemDetail {
    #[serde(flatten)]
    pub item: InboxItemSummary,
    pub files: Vec<InboxItemFileEntry>,
    pub transfer: Option<TransferProjection>,
}

impl From<entity::inbox_item_file::ModelEx> for InboxItemFileEntry {
    fn from(file: entity::inbox_item_file::ModelEx) -> Self {
        Self {
            id: file.id,
            transfer_file_id: file.transfer_file_id,
            relative_path: file.relative_path,
            name: file.name,
            size: file.size,
            checksum: file.checksum,
            local_path: file.local_path,
            missing: file.missing,
        }
    }
}

impl From<&entity::inbox_item::ModelEx> for InboxItemSummary {
    fn from(item: &entity::inbox_item::ModelEx) -> Self {
        Self {
            id: item.id,
            transfer_session_id: item.transfer_session_id,
            source_peer_id: item.source_peer_id.0.clone(),
            source_name: item.source_name.clone(),
            source_kind: item.source_kind.clone(),
            content_kind: item.content_kind.clone(),
            title: item.title.clone(),
            item_count: item.item_count,
            total_size: item.total_size,
            root_path: item.root_path.clone(),
            content_hash: item.content_hash.clone(),
            received_at: item.received_at,
            last_opened_at: item.last_opened_at,
            archived_at: item.archived_at,
            deleted_at: item.deleted_at,
            missing: item.files.iter().any(|file| file.missing),
        }
    }
}

impl InboxItemDetail {
    async fn from_model(
        db: &DatabaseConnection,
        item: entity::inbox_item::ModelEx,
    ) -> AppResult<Self> {
        let transfer = match item.transfer_session_id {
            Some(session_id) => get_transfer_projection(db, session_id).await?,
            None => None,
        };
        let files = item.files.clone().into_iter().map(Into::into).collect();
        Ok(Self {
            item: InboxItemSummary::from(&item),
            files,
            transfer,
        })
    }
}

/// 从已完成接收会话幂等创建收件箱条目。
///
/// 非 receive、未完成、失败/暂停/取消会话返回 `Ok(None)`，不会创建内容记录。
pub async fn ensure_inbox_item_for_completed_receive_session(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Option<InboxItemDetail>> {
    if let Some(existing) = find_inbox_item_by_session(db, session_id).await? {
        return Ok(Some(InboxItemDetail::from_model(db, existing).await?));
    }

    let session = entity::TransferSession::load()
        .filter_by_id(session_id)
        .with(entity::TransferFile)
        .one(db)
        .await?
        .ok_or_else(|| crate::AppError::Transfer("传输会话不存在".into()))?;

    let is_completed_receive = session.direction == TransferDirection::Receive
        && session.phase == TransferPhase::Terminal
        && session.terminal_reason == Some(TerminalReason::Completed);
    if !is_completed_receive {
        return Ok(None);
    }

    let Some(save_path) = session.save_path.clone().map(CoreSaveLocation::from) else {
        return Err(crate::AppError::Transfer(
            "已完成接收会话缺少保存位置，无法创建收件箱条目".into(),
        ));
    };

    let inbox_id = Uuid::new_v4();
    let files: Vec<&entity::transfer_file::ModelEx> = session.files.iter().collect();
    let item_count = i32::try_from(files.len())
        .map_err(|_| crate::AppError::Transfer("收件箱文件数量超出可表示范围".into()))?;
    let title = inbox_title(&files);
    let root_path = save_location_root(&save_path);
    let content_hash = inbox_content_hash(&files);
    let now = now_ms();

    let txn = db.begin().await?;

    entity::inbox_item::ActiveModel::builder()
        .set_id(inbox_id)
        .set_transfer_session_id(Some(session.session_id))
        .set_source_peer_id(session.peer_id.clone())
        .set_source_name(session.peer_name.clone())
        .set_source_kind(InboxSourceKind::PairedDevice)
        .set_content_kind(InboxContentKind::Files)
        .set_title(title)
        .set_item_count(item_count)
        .set_total_size(session.total_size)
        .set_root_path(root_path)
        .set_content_hash(Some(content_hash))
        .set_received_at(session.finished_at.unwrap_or(now))
        .insert(&txn)
        .await?;

    for file in files {
        entity::inbox_item_file::ActiveModel::builder()
            .set_inbox_item_id(inbox_id)
            .set_transfer_file_id(Some(file.id))
            .set_relative_path(file.relative_path.clone())
            .set_name(file.name.clone())
            .set_size(file.size)
            .set_checksum(file.checksum.clone())
            .set_local_path(resolve_local_path(&save_path, &file.relative_path))
            .set_missing(false)
            .insert(&txn)
            .await?;
    }

    txn.commit().await?;
    get_inbox_item_detail(db, inbox_id).await
}

/// 补建所有已完成 receive 会话缺失的收件箱条目。
pub async fn repair_missing_inbox_items_for_completed_receives(
    db: &DatabaseConnection,
) -> AppResult<Vec<InboxItemDetail>> {
    let sessions = entity::TransferSession::find()
        .filter(entity::transfer_session::Column::Direction.eq(TransferDirection::Receive))
        .filter(entity::transfer_session::Column::Phase.eq(TransferPhase::Terminal))
        .filter(
            entity::transfer_session::Column::TerminalReason.eq(Some(TerminalReason::Completed)),
        )
        .all(db)
        .await?;

    let mut repaired = Vec::new();
    for session in sessions {
        if find_inbox_item_by_session(db, session.session_id)
            .await?
            .is_none()
            && let Some(detail) =
                ensure_inbox_item_for_completed_receive_session(db, session.session_id).await?
        {
            repaired.push(detail);
        }
    }
    Ok(repaired)
}

/// 列出收件箱条目，默认由命令层传入是否包含归档项。
pub async fn list_inbox_items(
    db: &DatabaseConnection,
    include_archived: bool,
) -> AppResult<Vec<InboxItemSummary>> {
    let mut query = entity::InboxItem::load()
        .with(entity::InboxItemFile)
        .filter(entity::inbox_item::Column::DeletedAt.is_null())
        .order_by_desc(entity::inbox_item::Column::ReceivedAt);
    if !include_archived {
        query = query.filter(entity::inbox_item::Column::ArchivedAt.is_null());
    }
    Ok(query
        .all(db)
        .await?
        .iter()
        .map(InboxItemSummary::from)
        .collect())
}

/// 加载收件箱详情。软删除后的条目对普通详情不可见。
pub async fn get_inbox_item_detail(
    db: &DatabaseConnection,
    item_id: Uuid,
) -> AppResult<Option<InboxItemDetail>> {
    let Some(item) = entity::InboxItem::load()
        .filter_by_id(item_id)
        .filter(entity::inbox_item::Column::DeletedAt.is_null())
        .with(entity::InboxItemFile)
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(InboxItemDetail::from_model(db, item).await?))
}

/// 标记收件箱条目最近打开时间。
pub async fn mark_inbox_item_opened(db: &DatabaseConnection, item_id: Uuid) -> AppResult<()> {
    if let Some(item) = entity::InboxItem::find_by_id(item_id).one(db).await? {
        let mut model = item.into_active_model();
        model.last_opened_at = Set(Some(now_ms()));
        model.update(db).await?;
    }
    Ok(())
}

/// 归档或取消归档收件箱条目。
pub async fn archive_inbox_item(
    db: &DatabaseConnection,
    item_id: Uuid,
    archived: bool,
) -> AppResult<()> {
    if let Some(item) = entity::InboxItem::find_by_id(item_id).one(db).await? {
        let mut model = item.into_active_model();
        model.archived_at = Set(archived.then(now_ms));
        model.update(db).await?;
    }
    Ok(())
}

/// 软删除收件箱记录；是否删除本地文件由 host command 在调用前完成。
pub async fn delete_inbox_item_record(db: &DatabaseConnection, item_id: Uuid) -> AppResult<()> {
    if let Some(item) = entity::InboxItem::find_by_id(item_id).one(db).await? {
        let mut model = item.into_active_model();
        model.deleted_at = Set(Some(now_ms()));
        model.update(db).await?;
    }
    Ok(())
}

/// 标记收件箱文件缺失状态。
pub async fn mark_inbox_item_file_missing(
    db: &DatabaseConnection,
    file_id: i32,
    missing: bool,
) -> AppResult<()> {
    if let Some(file) = entity::InboxItemFile::find_by_id(file_id).one(db).await? {
        let mut model = file.into_active_model();
        model.missing = Set(missing);
        model.update(db).await?;
    }
    Ok(())
}

async fn find_inbox_item_by_session(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Option<entity::inbox_item::ModelEx>> {
    Ok(entity::InboxItem::load()
        .filter(entity::inbox_item::Column::TransferSessionId.eq(session_id))
        .with(entity::InboxItemFile)
        .one(db)
        .await?)
}

fn inbox_title(files: &[&entity::transfer_file::ModelEx]) -> String {
    match files {
        [] => "空传输".to_string(),
        [file] => file.name.clone(),
        [first, ..] => format!("{} 等 {} 个文件", first.name, files.len()),
    }
}

fn inbox_content_hash(files: &[&entity::transfer_file::ModelEx]) -> String {
    let mut hasher = blake3::Hasher::new();
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update(&[0]);
        hasher.update(file.checksum.as_bytes());
        hasher.update(&file.size.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn save_location_root(save_path: &CoreSaveLocation) -> Option<String> {
    match save_path {
        CoreSaveLocation::Path { path } => Some(path.clone()),
    }
}

fn resolve_local_path(save_path: &CoreSaveLocation, relative_path: &str) -> String {
    match save_path {
        CoreSaveLocation::Path { path } => std::path::Path::new(path)
            .join(relative_path)
            .to_string_lossy()
            .into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};

    use crate::database::ops::{
        CreateSessionInput, clear_all_history, create_session, mark_session_completed,
    };
    use crate::protocol::FileInfo;
    use crate::transfer::coordinator::TransferState;

    async fn make_db() -> DatabaseConnection {
        let mut opt = ConnectOptions::new("sqlite::memory:");
        opt.max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let db = Database::connect(opt).await.expect("connect sqlite memory");
        Migrator::up(&db, None).await.expect("run migrations");
        db
    }

    fn file_info(id: u32, relative_path: &str, size: u64) -> FileInfo {
        FileInfo {
            file_id: id,
            name: relative_path
                .rsplit('/')
                .next()
                .unwrap_or(relative_path)
                .to_string(),
            relative_path: relative_path.to_string(),
            size,
            checksum: format!("checksum-{id}"),
        }
    }

    async fn create_receive_session(
        db: &DatabaseConnection,
        session_id: Uuid,
        lifecycle: TransferState,
    ) {
        let files = vec![
            file_info(0, "hello.txt", 12),
            file_info(1, "docs/readme.md", 8),
        ];
        create_session(
            db,
            CreateSessionInput {
                session_id,
                direction: TransferDirection::Receive,
                peer_id: "peer-a",
                peer_name: "小明的 Mac",
                files: &files,
                total_size: 20,
                save_path: Some(CoreSaveLocation::Path {
                    path: "/tmp/swarmdrop-inbox-test".to_string(),
                }),
                source_paths: None,
                lifecycle,
            },
        )
        .await
        .expect("create receive session");
    }

    #[tokio::test]
    async fn completed_receive_should_create_inbox_item_idempotently() {
        let db = make_db().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&db, session_id, TransferState::active(0)).await;
        mark_session_completed(&db, session_id)
            .await
            .expect("mark completed");

        let first = ensure_inbox_item_for_completed_receive_session(&db, session_id)
            .await
            .expect("create inbox item")
            .expect("inbox item");
        let second = ensure_inbox_item_for_completed_receive_session(&db, session_id)
            .await
            .expect("create inbox item again")
            .expect("same inbox item");

        assert_eq!(first.item.id, second.item.id);
        assert_eq!(first.item.transfer_session_id, Some(session_id));
        assert_eq!(first.item.content_kind, InboxContentKind::Files);
        assert_eq!(first.item.item_count, 2);
        assert_eq!(first.files.len(), 2);
        assert!(first.files.iter().all(|file| !file.missing));

        let list = list_inbox_items(&db, false).await.expect("list inbox");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, first.item.id);
    }

    #[tokio::test]
    async fn unfinished_receive_should_not_create_inbox_item() {
        let db = make_db().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&db, session_id, TransferState::offered(0)).await;

        let item = ensure_inbox_item_for_completed_receive_session(&db, session_id)
            .await
            .expect("ensure inbox item");
        assert!(item.is_none());
        assert!(list_inbox_items(&db, false).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn clear_history_should_keep_inbox_records() {
        let db = make_db().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&db, session_id, TransferState::active(0)).await;
        mark_session_completed(&db, session_id).await.unwrap();
        let item = ensure_inbox_item_for_completed_receive_session(&db, session_id)
            .await
            .unwrap()
            .unwrap();

        clear_all_history(&db)
            .await
            .expect("clear activity history");

        let list = list_inbox_items(&db, false).await.expect("list inbox");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, item.item.id);
        assert!(
            get_transfer_projection(&db, session_id)
                .await
                .expect("query projection")
                .is_none()
        );
    }
}
