//! Drop Inbox 数据访问。
//!
//! 收件箱是“已接收内容索引”，与 transfer_sessions / transfer_files 的过程账本分开维护。

use entity::{InboxContentKind, InboxSourceKind, TerminalReason, TransferDirection, TransferPhase};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend,
    EntityLoaderTrait, EntityTrait, FromQueryResult, IntoActiveModel, QueryFilter, QueryOrder, Set,
    Statement, TransactionTrait,
};
use uuid::Uuid;

use crate::AppResult;
use crate::database::ops::{TransferProjection, get_transfer_projection, now_ms};

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

/// 收件箱搜索命中（item 粒度）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxSearchHit {
    pub id: Uuid,
    pub title: String,
    pub source_name: String,
    pub item_count: i32,
    pub root_path: Option<String>,
    pub received_at: i64,
    /// 命中所在文本的片段（在 Rust 端按子串位置切窗口生成）。
    pub snippet: String,
    /// 该条目下的文件（文件名 + 相对路径），供 get_inbox_file 下钻。
    pub files: Vec<InboxHitFile>,
}

/// 搜索命中条目下的文件标识（供下钻定位）。
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InboxHitFile {
    pub name: String,
    pub relative_path: String,
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

    // 已完成接收必有保存位置(不变量);缺失是数据异常,显式报错。save_root 仅作
    // content_root_of 返回 None(跨多目录)时的回退根。
    let Some(entity::SaveLocation::Path { path: save_root }) = session.save_path.as_ref() else {
        return Err(crate::AppError::Transfer(
            "已完成接收会话缺少保存位置，无法创建收件箱条目".into(),
        ));
    };

    let inbox_id = Uuid::new_v4();
    let files: Vec<&entity::transfer_file::ModelEx> = session.files.iter().collect();
    let item_count = i32::try_from(files.len())
        .map_err(|_| crate::AppError::Transfer("收件箱文件数量超出可表示范围".into()))?;
    let title = inbox_title(&files);
    // root_path = 真实容器目录(与传输投影 content_root 同一纯事实计算),None(跨多目录)
    // 时回退存储根 —— 收件箱前端只拿得到 rootPath、拿不到 saveLocation,兜底落 core 这侧。
    let root_path = crate::database::ops::content_root_of(session.files.iter())
        .or_else(|| Some(save_root.clone()));
    let content_hash = inbox_content_hash(&files);
    let now = now_ms();

    // FTS 聚合文本：该 item 所有文件名 + 相对路径空格拼接（与迁移回填 SQL 语义一致）。
    let files_text = files
        .iter()
        .map(|file| format!("{} {}", file.name, file.relative_path))
        .collect::<Vec<_>>()
        .join(" ");

    // 由会话发起来源派生：MCP/代理来源记为 Mcp，否则 PairedDevice。
    let source_kind = source_kind_for_origin(session.origin.as_deref());

    let txn = db.begin().await?;

    entity::inbox_item::ActiveModel::builder()
        .set_id(inbox_id)
        .set_transfer_session_id(Some(session.session_id))
        .set_source_peer_id(session.peer_id.clone())
        .set_source_name(session.peer_name.clone())
        .set_source_kind(source_kind)
        .set_content_kind(InboxContentKind::Files)
        .set_title(title.clone())
        .set_item_count(item_count)
        .set_total_size(session.total_size)
        .set_root_path(root_path)
        .set_content_hash(Some(content_hash))
        .set_received_at(session.finished_at.unwrap_or(now))
        .insert(&txn)
        .await?;

    for file in files {
        // finalize_sink 记录的最终落盘位置是唯一事实源（SAF document URI /
        // 重名冲突改写都无法由「目录 + 相对路径」拼接推导）。已完成接收会话的
        // 文件必然写过它——缺失即数据异常（如旧版本残留库），显式报错不做推导。
        let Some(local_path) = file.local_path.clone() else {
            txn.rollback().await?;
            return Err(crate::AppError::Transfer(format!(
                "已完成接收文件缺少落盘路径记录: {}（旧版本数据，请清除应用数据后重试）",
                file.name
            )));
        };
        entity::inbox_item_file::ActiveModel::builder()
            .set_inbox_item_id(inbox_id)
            .set_transfer_file_id(Some(file.id))
            .set_relative_path(file.relative_path.clone())
            .set_name(file.name.clone())
            .set_size(file.size)
            .set_checksum(file.checksum.clone())
            .set_local_path(local_path)
            .set_missing(false)
            .insert(&txn)
            .await?;
    }

    // inline 维护 FTS 索引：item + 全部 file 已在同一事务内，一次写入聚合行。
    txn.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO inbox_fts(item_id, title, source_name, files_text, extracted_text) \
         VALUES(?, ?, ?, ?, '')",
        [
            inbox_id.into(),
            title.into(),
            session.peer_name.clone().into(),
            files_text.into(),
        ],
    ))
    .await?;

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
        {
            // 尽力补建：单个会话失败（如 local_path 为 NULL 的旧数据）只跳过，
            // 不掐断整批——否则一个坏会话会让其后所有可补会话永远建不出来。
            match ensure_inbox_item_for_completed_receive_session(db, session.session_id).await {
                Ok(Some(detail)) => repaired.push(detail),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        "补建收件箱条目失败，跳过: session={}, {e}",
                        session.session_id
                    );
                }
            }
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

/// FTS 命中的 item_id（仅用于保留按接收时间倒序的命中顺序）。
///
/// inbox_items.id 在 SQLite 里以 BLOB 存储，FTS 的 item_id 也存同一 BLOB，按 Uuid 解码。
#[derive(FromQueryResult)]
struct InboxFtsHitId {
    item_id: Uuid,
}

/// inbox 子串检索：以 item 为粒度，按接收时间倒序，截断到 `limit`。
///
/// 不使用 FTS5 MATCH/bm25（trigram 对 <3 字查询无法命中，会让"合同"这类 2 字中文词返回空），
/// 统一对索引文本列做 `LIKE` 子串匹配：≥3 字经 trigram 索引加速、更短查询退化为全表扫描但结果正确。
/// 排除软删条目；`include_archived=false` 时排除已归档项。
pub async fn search_inbox(
    db: &DatabaseConnection,
    query: &str,
    limit: usize,
    include_archived: bool,
) -> AppResult<Vec<InboxSearchHit>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let pattern = format!("%{}%", escape_like(trimmed));
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let include_archived = i64::from(include_archived);

    // 第一步：FTS 子串匹配，取回按接收时间倒序的命中 item_id。
    let ordered = InboxFtsHitId::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        r#"
        SELECT fts.item_id AS item_id
        FROM inbox_fts AS fts
        JOIN inbox_items AS i ON i.id = fts.item_id
        WHERE i.deleted_at IS NULL
          AND (? = 1 OR i.archived_at IS NULL)
          AND (
              fts.title LIKE ? ESCAPE '\'
              OR fts.source_name LIKE ? ESCAPE '\'
              OR fts.files_text LIKE ? ESCAPE '\'
              OR fts.extracted_text LIKE ? ESCAPE '\'
          )
        ORDER BY i.received_at DESC
        LIMIT ?
        "#,
        [
            include_archived.into(),
            pattern.clone().into(),
            pattern.clone().into(),
            pattern.clone().into(),
            pattern.into(),
            limit.into(),
        ],
    ))
    .all(db)
    .await?;

    if ordered.is_empty() {
        return Ok(Vec::new());
    }

    // 第二步：按 item 加载详情（含文件），entity loader 复用既有模式。
    let ids: Vec<Uuid> = ordered.iter().map(|hit| hit.item_id).collect();
    let mut by_id: std::collections::HashMap<Uuid, entity::inbox_item::ModelEx> =
        entity::InboxItem::load()
            .with(entity::InboxItemFile)
            .filter(entity::inbox_item::Column::Id.is_in(ids))
            .all(db)
            .await?
            .into_iter()
            .map(|item| (item.id, item))
            .collect();

    // 第三步：按 FTS 命中顺序组装结果并生成片段。
    let mut hits = Vec::with_capacity(ordered.len());
    for hit in &ordered {
        if let Some(item) = by_id.remove(&hit.item_id) {
            hits.push(build_search_hit(item, trimmed));
        }
    }
    Ok(hits)
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

/// 由接收会话的 `origin` 列派生收件箱 `source_kind`：MCP/代理来源 → `Mcp`，否则 `PairedDevice`。
/// 历史 NULL / 未知值经 `TransferOrigin::from_db_string` 回退 `Human` → `PairedDevice`。
fn source_kind_for_origin(origin: Option<&str>) -> InboxSourceKind {
    match crate::protocol::TransferOrigin::from_db_string(origin.unwrap_or("human")) {
        crate::protocol::TransferOrigin::Mcp { .. } => InboxSourceKind::Mcp,
        crate::protocol::TransferOrigin::Human => InboxSourceKind::PairedDevice,
    }
}

/// 转义 LIKE 通配符（`\` `%` `_`），避免用户输入被当成通配模式（配合 SQL 的 `ESCAPE '\'`）。
fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn build_search_hit(item: entity::inbox_item::ModelEx, query: &str) -> InboxSearchHit {
    let files: Vec<InboxHitFile> = item
        .files
        .iter()
        .map(|file| InboxHitFile {
            name: file.name.clone(),
            relative_path: file.relative_path.clone(),
        })
        .collect();
    let snippet = make_snippet(query, &item, &files);
    InboxSearchHit {
        id: item.id,
        title: item.title.clone(),
        source_name: item.source_name.clone(),
        item_count: item.item_count,
        root_path: item.root_path.clone(),
        received_at: item.received_at,
        snippet,
        files,
    }
}

/// 在标题 / 来源名 / 文件文本里找首个命中子串，按字符切窗口生成片段（UTF-8 安全）。
fn make_snippet(query: &str, item: &entity::inbox_item::ModelEx, files: &[InboxHitFile]) -> String {
    let needle = query.to_lowercase();
    let mut candidates: Vec<String> = vec![item.title.clone(), item.source_name.clone()];
    for file in files {
        candidates.push(format!("{} {}", file.name, file.relative_path));
    }
    for text in &candidates {
        if let Some(snippet) = snippet_window(text, &needle) {
            return snippet;
        }
    }
    item.title.clone()
}

fn snippet_window(text: &str, needle_lower: &str) -> Option<String> {
    let hay = text.to_lowercase();
    let byte_pos = hay.find(needle_lower)?;
    let char_start = hay[..byte_pos].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let needle_len = needle_lower.chars().count();
    const CTX: usize = 16;
    let start = char_start.saturating_sub(CTX);
    let end = (char_start + needle_len + CTX).min(chars.len());
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(chars[start..end].iter());
    if end < chars.len() {
        out.push('…');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};

    use crate::database::ops::{
        CreateSessionInput, clear_all_history, create_session, mark_file_completed,
        mark_session_completed,
    };
    use crate::host::CoreSaveLocation;
    use crate::protocol::FileInfo;
    use crate::transfer::coordinator::TransferState;

    /// 模拟 receiver 的文件级完成：真实链路里 finalize_sink 的返回值经
    /// `mark_file_completed` 写入 local_path，收件箱落库依赖它。
    async fn mark_files_completed(db: &DatabaseConnection, session_id: Uuid, files: &[FileInfo]) {
        for file in files {
            let local_path = format!("/tmp/swarmdrop-inbox-test/{}", file.relative_path);
            // 父目录 = local_path 的 dirname(模拟 finalize_sink 的 dir 返回)。
            let local_dir = local_path
                .rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            mark_file_completed(
                db,
                session_id,
                file.file_id as i32,
                vec![],
                file.size as i64,
                local_path,
                local_dir,
            )
            .await
            .expect("mark file completed");
        }
    }

    #[test]
    fn source_kind_derived_from_origin() {
        assert!(matches!(
            source_kind_for_origin(None),
            InboxSourceKind::PairedDevice
        ));
        assert!(matches!(
            source_kind_for_origin(Some("human")),
            InboxSourceKind::PairedDevice
        ));
        assert!(matches!(
            source_kind_for_origin(Some("mcp")),
            InboxSourceKind::Mcp
        ));
        assert!(matches!(
            source_kind_for_origin(Some("mcp:claude-desktop")),
            InboxSourceKind::Mcp
        ));
    }

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
                policy: None,
                origin: None,
            },
        )
        .await
        .expect("create receive session");
        mark_files_completed(db, session_id, &files).await;
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

    /// 创建一个已完成接收会话并落库为收件箱条目，返回 item id。
    async fn make_inbox_item(db: &DatabaseConnection, peer_name: &str, files: &[FileInfo]) -> Uuid {
        let session_id = Uuid::new_v4();
        let total: u64 = files.iter().map(|file| file.size).sum();
        create_session(
            db,
            CreateSessionInput {
                session_id,
                direction: TransferDirection::Receive,
                peer_id: "peer-search",
                peer_name,
                files,
                total_size: total,
                save_path: Some(CoreSaveLocation::Path {
                    path: "/tmp/swarmdrop-inbox-search-test".to_string(),
                }),
                source_paths: None,
                lifecycle: TransferState::active(0),
                policy: None,
                origin: None,
            },
        )
        .await
        .expect("create receive session");
        mark_files_completed(db, session_id, files).await;
        mark_session_completed(db, session_id)
            .await
            .expect("mark completed");
        ensure_inbox_item_for_completed_receive_session(db, session_id)
            .await
            .expect("ensure inbox item")
            .expect("inbox item")
            .item
            .id
    }

    async fn set_received_at(db: &DatabaseConnection, id: Uuid, ts: i64) {
        let item = entity::InboxItem::find_by_id(id)
            .one(db)
            .await
            .unwrap()
            .unwrap();
        let mut model = item.into_active_model();
        model.received_at = Set(ts);
        model.update(db).await.unwrap();
    }

    #[tokio::test]
    async fn search_finds_item_by_title_and_source_after_insert() {
        let db = make_db().await;
        let id = make_inbox_item(
            &db,
            "Alice 的工作站",
            &[file_info(0, "季度合同扫描.pdf", 12)],
        )
        .await;

        let by_title = search_inbox(&db, "扫描", 10, false).await.unwrap();
        assert!(by_title.iter().any(|h| h.id == id), "标题/文件名应命中");

        let by_source = search_inbox(&db, "Alice", 10, false).await.unwrap();
        assert!(by_source.iter().any(|h| h.id == id), "来源设备名应命中");

        let hit = by_title.iter().find(|h| h.id == id).unwrap();
        assert_eq!(hit.files.len(), 1);
        assert_eq!(hit.files[0].relative_path, "季度合同扫描.pdf");
        assert!(!hit.snippet.is_empty(), "命中应带匹配片段");
    }

    #[tokio::test]
    async fn search_cjk_two_char_word_matches() {
        let db = make_db().await;
        let id = make_inbox_item(&db, "Bob", &[file_info(0, "合同.pdf", 8)]).await;
        // 招牌回归点：2 字中文词在 trigram MATCH 下会返回空，必须靠 LIKE 兜底命中。
        let hits = search_inbox(&db, "合同", 10, false).await.unwrap();
        assert!(
            hits.iter().any(|h| h.id == id),
            "2 字中文词 '合同' 必须命中，不能返回空"
        );
    }

    #[tokio::test]
    async fn search_excludes_deleted_and_archived_by_default() {
        let db = make_db().await;
        let kept = make_inbox_item(&db, "Carol", &[file_info(0, "报告.pdf", 5)]).await;
        let deleted = make_inbox_item(&db, "Carol", &[file_info(1, "报告草稿.pdf", 5)]).await;
        let archived = make_inbox_item(&db, "Carol", &[file_info(2, "报告归档.pdf", 5)]).await;

        delete_inbox_item_record(&db, deleted).await.unwrap();
        archive_inbox_item(&db, archived, true).await.unwrap();

        let default_ids: Vec<Uuid> = search_inbox(&db, "报告", 10, false)
            .await
            .unwrap()
            .iter()
            .map(|h| h.id)
            .collect();
        assert!(default_ids.contains(&kept));
        assert!(!default_ids.contains(&deleted), "软删条目不应返回");
        assert!(!default_ids.contains(&archived), "默认不返回已归档条目");

        let with_archived_ids: Vec<Uuid> = search_inbox(&db, "报告", 10, true)
            .await
            .unwrap()
            .iter()
            .map(|h| h.id)
            .collect();
        assert!(
            with_archived_ids.contains(&archived),
            "include_archived 时应返回已归档条目"
        );
        assert!(!with_archived_ids.contains(&deleted), "软删条目始终不返回");
    }

    #[tokio::test]
    async fn search_orders_by_received_at_desc_and_respects_limit() {
        let db = make_db().await;
        let a = make_inbox_item(&db, "Dave", &[file_info(0, "票据A.pdf", 5)]).await;
        let b = make_inbox_item(&db, "Dave", &[file_info(1, "票据B.pdf", 5)]).await;
        let c = make_inbox_item(&db, "Dave", &[file_info(2, "票据C.pdf", 5)]).await;
        set_received_at(&db, a, 100).await;
        set_received_at(&db, b, 200).await;
        set_received_at(&db, c, 300).await;

        let hits = search_inbox(&db, "票据", 2, false).await.unwrap();
        assert_eq!(hits.len(), 2, "limit 应截断到 2");
        assert_eq!(hits[0].id, c, "最新（received_at 最大）排最前");
        assert_eq!(hits[1].id, b);
    }

    #[tokio::test]
    async fn search_empty_query_and_no_match_return_empty() {
        let db = make_db().await;
        make_inbox_item(&db, "Erin", &[file_info(0, "发票.pdf", 5)]).await;
        assert!(search_inbox(&db, "", 10, false).await.unwrap().is_empty());
        assert!(
            search_inbox(&db, "   ", 10, false)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            search_inbox(&db, "不存在的关键词zzz", 10, false)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn backfill_repopulates_fts_for_existing_items() {
        let db = make_db().await;
        let id = make_inbox_item(&db, "Frank", &[file_info(0, "合同附件.pdf", 5)]).await;

        // 模拟"有数据但无索引"的存量库
        db.execute_unprepared("DELETE FROM inbox_fts")
            .await
            .unwrap();
        assert!(
            search_inbox(&db, "合同", 10, false)
                .await
                .unwrap()
                .is_empty(),
            "清空索引后应搜不到"
        );

        // 执行与迁移等价的回填 SQL
        db.execute_unprepared(
            r#"
            INSERT INTO inbox_fts(item_id, title, source_name, files_text, extracted_text)
            SELECT i.id, i.title, i.source_name,
                   COALESCE(group_concat(f.name || ' ' || f.relative_path, ' '), ''), ''
            FROM inbox_items i
            LEFT JOIN inbox_item_files f ON f.inbox_item_id = i.id
            GROUP BY i.id, i.title, i.source_name
            "#,
        )
        .await
        .unwrap();

        let hits = search_inbox(&db, "合同", 10, false).await.unwrap();
        assert!(hits.iter().any(|h| h.id == id), "回填后历史条目应可搜");
    }

    #[tokio::test]
    async fn search_works_with_empty_extracted_text() {
        let db = make_db().await;
        let id = make_inbox_item(&db, "Grace", &[file_info(0, "笔记.txt", 5)]).await;
        // 所有 extracted_text 为空：仅靠标题/来源名/文件名仍能命中，且无关词不凭空命中。
        assert!(
            search_inbox(&db, "笔记", 10, false)
                .await
                .unwrap()
                .iter()
                .any(|h| h.id == id)
        );
        assert!(
            search_inbox(&db, "OCR内容xyz", 10, false)
                .await
                .unwrap()
                .is_empty(),
            "extracted_text 为空，不应凭空命中"
        );
    }
}
