//! Drop Inbox 数据访问（[`swarmdrop_transfer::store::InboxStore`] 的 SeaORM 实现体）。
//!
//! 收件箱是“已接收内容索引”，与 transfer_sessions / transfer_files 的过程账本分开维护。
//!
//! 本文件的函数**全部私有**：唯一入口是 `crate::store` 里 `SqlSessionStore` 的
//! `InboxStore` impl。DTO 与共享领域规则（标题 / 内容指纹 / 聚合文本 / 来源分类 /
//! 片段）住在 [`swarmdrop_transfer::inbox`]，本文件只负责「SQL 行 ↔ 中立 DTO」的
//! 转换与查询本身。

use entity::{InboxContentKind, TerminalReason, TransferDirection, TransferPhase};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbBackend, EntityLoaderTrait, EntityTrait,
    FromQueryResult, IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, Set, Statement,
    TransactionTrait, sea_query::Expr,
};
use uuid::Uuid;

use crate::ops::{get_transfer_projection, now_ms};
use swarmdrop_host::{AppResult, CoreSaveLocation};
use swarmdrop_transfer::inbox::{
    InboxFileFacts, InboxHitFile, InboxItemDetail, InboxItemFileEntry, InboxItemSummary,
    InboxSearchHit, inbox_content_hash, inbox_files_text, inbox_primary_file_name, inbox_snippet,
    inbox_source_kind, is_completed_receive,
};

/// `ModelEx` → 中立 DTO。写成自由函数而非 `From` impl：两端类型都不属于本 crate
/// （DTO 在 swarmdrop-transfer、`ModelEx` 在 entity），孤儿规则不允许在这里 impl 外部 trait。
fn file_entry(file: entity::inbox_item_file::ModelEx) -> InboxItemFileEntry {
    InboxItemFileEntry {
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

/// 同上：`ModelEx` → 列表条目 DTO。`missing` 由文件行聚合而来，故必须吃带关系的 `ModelEx`。
fn item_summary(item: &entity::inbox_item::ModelEx) -> InboxItemSummary {
    InboxItemSummary {
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

/// 传输文件行 → 收件箱规则的中立视图。`ModelEx` 到此为止，不越过本 crate。
fn file_facts(file: &entity::transfer_file::ModelEx) -> InboxFileFacts<'_> {
    InboxFileFacts {
        name: &file.name,
        relative_path: &file.relative_path,
        checksum: &file.checksum,
        size: file.size,
    }
}

async fn detail_from_model(
    db: &DatabaseConnection,
    item: entity::inbox_item::ModelEx,
) -> AppResult<InboxItemDetail> {
    let transfer = match item.transfer_session_id {
        Some(session_id) => get_transfer_projection(db, session_id).await?,
        None => None,
    };
    let files = item.files.clone().into_iter().map(file_entry).collect();
    Ok(InboxItemDetail {
        item: item_summary(&item),
        files,
        transfer,
    })
}

/// 从已完成接收会话幂等创建收件箱条目。
///
/// 非 receive、未完成、失败/暂停/取消会话返回 `Ok(None)`，不会创建内容记录。
pub(crate) async fn ensure_inbox_item_for_completed_receive_session(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Option<InboxItemDetail>> {
    if let Some(existing) = find_inbox_item_by_session(db, session_id).await? {
        return Ok(Some(detail_from_model(db, existing).await?));
    }

    let mut loaded = entity::TransferSession::load()
        .filter_by_id(session_id)
        .with(entity::TransferFile)
        .one(db)
        .await?
        .ok_or_else(|| swarmdrop_host::AppError::SessionNotFound("传输会话不存在".into()))?;

    // 先把关系字段摘下来，再把 `ModelEx` 摊成纯 scalar 的 `Model`：共享判据
    // `is_completed_receive` 只吃 `Model`（关系机制编不进 wasm）。两步都是移动而非
    // 深拷——文件行里的 `completed_chunks` 位图与 `outboard` BLOB 不会被复制一份。
    // `HasMany` 只 Deref 到 `&[_]`，排序要先摊成 Vec；`into_iter` 是移动，不深拷。
    let mut file_rows: Vec<_> = std::mem::take(&mut loaded.files).into_iter().collect();
    // `load().with()` 不带 ORDER BY，行序是 SQLite 的实现细节（当前是 rowid 升序）。
    // 而下面三条领域规则里 `inbox_content_hash` 按顺序累加、`inbox_primary_file_name`
    // 取第 0 个——**跨端字节级契约靠一个未承诺的默认顺序兜着**。显式按主键排序钉死它；
    // 这不改变现有哈希（rowid 顺序本就等于 id 升序），只是把巧合变成保证。
    file_rows.sort_unstable_by_key(|file| file.id);
    let session: entity::transfer_session::Model = loaded.into();

    // 「哪些会话进收件箱」是跨端共享的一条判据（接收 + 终态 + 完成），三处存储实现
    // 一律调 swarmdrop_transfer::inbox 的那一份，不在这里重写三段合取。
    if !is_completed_receive(&session) {
        return Ok(None);
    }

    // 已完成接收必有保存位置(不变量);缺失是数据异常,显式报错。容器目录(含缺
    // local_dir 时回退存储根)由下面 content_root_of 统一解析。
    if session.save_path.is_none() {
        return Err(swarmdrop_host::AppError::Transfer(
            "已完成接收会话缺少保存位置，无法创建收件箱条目".into(),
        ));
    }

    let inbox_id = Uuid::new_v4();
    let item_count = i32::try_from(file_rows.len())
        .map_err(|_| swarmdrop_host::AppError::Transfer("收件箱文件数量超出可表示范围".into()))?;
    // 标题 / 内容指纹 / 检索聚合文本三条都是**跨端共享的领域规则**，一律调
    // swarmdrop_transfer::inbox 的那一份，本文件只负责把行类型摊成中立视图。
    let facts: Vec<InboxFileFacts<'_>> = file_rows.iter().map(file_facts).collect();
    let title = inbox_primary_file_name(&facts);
    // root_path = 真实容器目录(与传输投影 content_root 同一 core 解析:缺 local_dir 时
    // 回退存储根)。兜底收口在 content_root_of 一处,不再重复;它只读 `local_dir` 一列,
    // 故直接把这一列的迭代器递过去——为读一个字段深拷整份文件行,等于把位图与 outboard
    // BLOB 白复制一遍(每完成一次接收数百 KB～数 MB)。
    let save_location = session.save_path.clone().map(CoreSaveLocation::from);
    let root_path = swarmdrop_transfer::store::content_root_of(
        file_rows.iter().map(|f| f.local_dir.as_deref()),
        save_location.as_ref(),
    );
    let content_hash = inbox_content_hash(&facts);
    let now = now_ms();

    // 检索聚合文本：该 item 所有文件名 + 相对路径空格拼接。
    let files_text = inbox_files_text(&facts);

    // 由会话发起来源派生：MCP/代理来源记为 Mcp，否则 PairedDevice。
    let source_kind = inbox_source_kind(session.origin.as_deref());

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

    for file in file_rows.iter() {
        // finalize_sink 记录的最终落盘位置是唯一事实源（SAF document URI /
        // 重名冲突改写都无法由「目录 + 相对路径」拼接推导）。已完成接收会话的
        // 文件必然写过它——缺失即数据异常（如旧版本残留库），显式报错不做推导。
        let Some(local_path) = file.local_path.clone() else {
            txn.rollback().await?;
            return Err(swarmdrop_host::AppError::Transfer(format!(
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

    // inline 维护检索索引：item + 全部 file 已在同一事务内，一次写入聚合行。
    //
    // **走 `Entity::insert(..).exec_without_returning`，不是 builder 的 `.insert(&txn)`。**
    // 后者会 `db.begin()`（在事务里 = 多一对 SAVEPOINT / RELEASE）并以
    // `INSERT ... RETURNING <全部列>` 把刚写进去的整行读回来立刻丢掉 —— 而 `files_text`
    // 是**无界**的（该条目每个文件的 `name + relative_path` 拼接，2000 个文件约 200 KB）。
    // 一条语句能办的事不值得付 3 条语句 + 一次全行回读。
    let index_row: entity::inbox_search_index::ActiveModel =
        entity::inbox_search_index::ActiveModel::builder()
            .set_item_id(inbox_id)
            .set_source_name(session.peer_name.clone())
            .set_files_text(files_text)
            // 预留给未来的 OCR / 文本抽取，这一端没有抽取能力，恒为空串。
            .set_extracted_text(String::new())
            .into();
    entity::InboxSearchIndex::insert(index_row)
        .exec_without_returning(&txn)
        .await?;

    txn.commit().await?;
    get_inbox_item_detail(db, inbox_id).await
}

/// 补建所有已完成 receive 会话缺失的收件箱条目。
pub(crate) async fn repair_missing_inbox_items_for_completed_receives(
    db: &DatabaseConnection,
) -> AppResult<Vec<InboxItemDetail>> {
    // 下面三个 filter 是 [`swarmdrop_transfer::inbox::is_completed_receive`] 的 SQL 手抄本
    // ——数据库里调不到 Rust 函数，只能复刻。**改那边必须同步改这里**，否则「哪些会话
    // 进收件箱」在扫描路径与建条目路径上会给出不同答案（表现是补建扫描漏掉 / 多捞会话，
    // 不报错）。只取主键：这里除了 session_id 什么都不用。
    let session_ids: Vec<Uuid> = entity::TransferSession::find()
        .select_only()
        .column(entity::transfer_session::Column::SessionId)
        .filter(entity::transfer_session::Column::Direction.eq(TransferDirection::Receive))
        .filter(entity::transfer_session::Column::Phase.eq(TransferPhase::Terminal))
        .filter(
            entity::transfer_session::Column::TerminalReason.eq(Some(TerminalReason::Completed)),
        )
        .into_tuple()
        .all(db)
        .await?;

    // 已有条目的会话一次查清，不逐会话查一遍：`ensure_*` 进门第一件事就是查重，
    // 外层再查一次等于每个会话两趟相同查询（且 `find_inbox_item_by_session` 会连
    // 文件行一起拉）。这层预筛不能省——`ensure_*` 对「新建」与「本来就有」都返回
    // `Ok(Some(detail))`，只靠它的返回值分不出本次到底补了什么。
    let already_indexed: std::collections::HashSet<Uuid> = entity::InboxItem::find()
        .select_only()
        .column(entity::inbox_item::Column::TransferSessionId)
        .filter(entity::inbox_item::Column::TransferSessionId.is_not_null())
        .into_tuple()
        .all(db)
        .await?
        .into_iter()
        .collect();

    let mut repaired = Vec::new();
    for session_id in session_ids {
        if already_indexed.contains(&session_id) {
            continue;
        }
        // 尽力补建：单个会话失败（如 local_path 为 NULL 的旧数据）只跳过，
        // 不掐断整批——否则一个坏会话会让其后所有可补会话永远建不出来。
        match ensure_inbox_item_for_completed_receive_session(db, session_id).await {
            Ok(Some(detail)) => repaired.push(detail),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("补建收件箱条目失败，跳过: session={session_id}, {e}");
            }
        }
    }
    Ok(repaired)
}

/// 列出收件箱条目，默认由命令层传入是否包含归档项。
pub(crate) async fn list_inbox_items(
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
    Ok(query.all(db).await?.iter().map(item_summary).collect())
}

/// 检索命中的 item_id（仅用于保留按接收时间倒序的命中顺序）。
///
/// `inbox_items.id` 在 SQLite 里以 BLOB 存储，索引表的 `item_id` 存同一 BLOB，按 Uuid 解码。
#[derive(FromQueryResult)]
struct InboxSearchHitId {
    item_id: Uuid,
}

/// inbox 子串检索：以 item 为粒度，按接收时间倒序，截断到 `limit`。
///
/// 对索引表的四个文本列做 `LIKE` 子串匹配。排除软删条目；
/// `include_archived=false` 时排除已归档项。
///
/// **判据从来不是 FTS5 的 `MATCH`。** 索引表一度是 FTS5 虚表（trigram 分词），但
/// trigram 对 <3 字的查询无法命中——「合同」这类 2 字中文词会返回空——所以命中一直
/// 由这里的 `LIKE` 承担，虚表只提供了一层 ≥3 字查询的索引加速。表改成普通表之后
/// 那层加速没了，查询退化为全表扫描；收件箱条目数与用户实际接收次数同阶，
/// 代价可忽略（Web 端本就是纯内存线性扫描）。
pub(crate) async fn search_inbox(
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

    // 第一步：子串匹配，取回按接收时间倒序的命中 item_id。
    //
    // 下面这段 `LIKE ... ESCAPE '\'` 是 [`swarmdrop_transfer::inbox::inbox_matches`]
    // 的 SQL 复刻：那个函数是检索命中判据的规范定义（大小写不敏感子串，覆盖
    // source_name / files_text / extracted_text 三列），Web 端直接调它。
    // 两者语义必须同义——同一个查询词在两端给出不同的命中集合，就是同一次搜索在
    // 两端结果不同。改这里必须同步改那里，反之亦然，而
    // `sql_like_matches_shared_corpus_same_as_inbox_matches` 用共享语料
    // `INBOX_MATCH_CASES` 把这句话钉成可执行的断言。
    let ordered = InboxSearchHitId::find_by_statement(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        r#"
        SELECT s.item_id AS item_id
        FROM inbox_search_index AS s
        JOIN inbox_items AS i ON i.id = s.item_id
        WHERE i.deleted_at IS NULL
          AND (? = 1 OR i.archived_at IS NULL)
          AND (
              s.source_name LIKE ? ESCAPE '\'
              OR s.files_text LIKE ? ESCAPE '\'
              OR s.extracted_text LIKE ? ESCAPE '\'
          )
        ORDER BY i.received_at DESC
        LIMIT ?
        "#,
        [
            include_archived.into(),
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
pub(crate) async fn get_inbox_item_detail(
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
    Ok(Some(detail_from_model(db, item).await?))
}

/// 加载与指定传输会话关联的可见收件箱详情。
///
/// 与幂等创建路径使用的 `find_inbox_item_by_session` 不同，这个查询面向 UI/API，
/// 会排除已软删除的收件箱条目。
pub(crate) async fn get_inbox_item_by_transfer_session_id(
    db: &DatabaseConnection,
    session_id: Uuid,
) -> AppResult<Option<InboxItemDetail>> {
    let Some(item) = entity::InboxItem::load()
        .filter(entity::inbox_item::Column::TransferSessionId.eq(session_id))
        .filter(entity::inbox_item::Column::DeletedAt.is_null())
        .with(entity::InboxItemFile)
        .one(db)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(detail_from_model(db, item).await?))
}

/// 标记收件箱条目最近打开时间。
pub(crate) async fn mark_inbox_item_opened(
    db: &DatabaseConnection,
    item_id: Uuid,
) -> AppResult<()> {
    if let Some(item) = entity::InboxItem::find_by_id(item_id).one(db).await? {
        let mut model = item.into_active_model();
        model.last_opened_at = Set(Some(now_ms()));
        model.update(db).await?;
    }
    Ok(())
}

/// 归档或取消归档收件箱条目。
pub(crate) async fn archive_inbox_item(
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
pub(crate) async fn delete_inbox_item_record(
    db: &DatabaseConnection,
    item_id: Uuid,
) -> AppResult<()> {
    if let Some(item) = entity::InboxItem::find_by_id(item_id).one(db).await? {
        let mut model = item.into_active_model();
        model.deleted_at = Set(Some(now_ms()));
        model.update(db).await?;
    }
    Ok(())
}

/// 标记收件箱文件缺失状态。
///
/// 归属校验就是 `WHERE` 的第二个条件：`file_id` 是 `inbox_item_files` 的全局自增主键，
/// 不带 `inbox_item_id` 一起过滤的话，传任意 `item_id` 都能改到别的条目下的文件。
/// 这条检查此前只有移动端桥接层做了，桌面没做——端口收口后由本实现统一承担
/// （见 `InboxStore` 的方法文档）。
///
/// 一条 UPDATE 顶掉「读整条条目（含全部文件行）→ 内存里找 → 再写回」的三趟查询：
/// 桌面 `delete_inbox_item(delete_local_files=true)` 会在文件循环里逐个调本函数，
/// 500 个文件按旧写法就是 500 × 3 趟、且每趟都把该条目的全部文件行读一遍。
pub(crate) async fn mark_inbox_item_file_missing(
    db: &DatabaseConnection,
    item_id: Uuid,
    file_id: i32,
    missing: bool,
) -> AppResult<()> {
    let updated = entity::InboxItemFile::update_many()
        .col_expr(
            entity::inbox_item_file::Column::Missing,
            Expr::value(missing),
        )
        .filter(entity::inbox_item_file::Column::Id.eq(file_id))
        .filter(entity::inbox_item_file::Column::InboxItemId.eq(item_id))
        .exec(db)
        .await?;
    // 0 行受影响 = 文件不存在 or 不属于该条目。两种情况本就都是「这个 (item, file)
    // 组合不成立」，对调用方是同一个错误。
    if updated.rows_affected == 0 {
        return Err(swarmdrop_host::AppError::Transfer(
            "收件箱文件不属于该条目".into(),
        ));
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

/// 转义 LIKE 通配符（`\` `%` `_`），避免用户输入被当成通配模式（配合 SQL 的 `ESCAPE '\'`）。
///
/// 不上提到 `swarmdrop_transfer::inbox`：它是 SQL `LIKE` 的通配语法细节，不是领域规则。
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
    let snippet = inbox_snippet(query, &item.title, &item.source_name, &files);
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};

    use crate::store::SqlSessionStore;
    use swarmdrop_transfer::coordinator::TransferState;
    use swarmdrop_transfer::protocol::FileInfo;
    use swarmdrop_transfer::store::{CreateSessionInput, InboxStore, SessionStore};

    /// 已完成接收的终态 fixture：收件箱落库只认 `phase=terminal + reason=completed`，
    /// 建会话时一次写到位，不再走额外的 `mark_*` 直写。
    fn completed_state() -> TransferState {
        TransferState {
            phase: TransferPhase::Terminal,
            suspended_reason: None,
            terminal_reason: Some(TerminalReason::Completed),
            epoch: 0,
            recoverable: false,
            failure: None,
        }
    }

    /// 模拟 receiver 的文件级完成：真实链路里 finalize_sink 的返回值经
    /// `mark_file_completed` 写入 local_path，收件箱落库依赖它。
    async fn mark_files_completed(store: &SqlSessionStore, session_id: Uuid, files: &[FileInfo]) {
        for file in files {
            let local_path = format!("/tmp/swarmdrop-inbox-test/{}", file.relative_path);
            // 父目录 = local_path 的 dirname(模拟 finalize_sink 的 dir 返回)。
            let local_dir = local_path
                .rsplit_once('/')
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            store
                .mark_file_completed(
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

    /// 测试一律经端口调用——被测的是「`InboxStore` 的行为」，不是私有函数的实现细节。
    /// 少数断言（改 `received_at`、直查行、灌 FTS）没有对应端口方法，那几处才用裸连接。
    async fn make_env() -> (DatabaseConnection, SqlSessionStore) {
        let mut opt = ConnectOptions::new("sqlite::memory:");
        opt.max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let db = Database::connect(opt).await.expect("connect sqlite memory");
        Migrator::up(&db, None).await.expect("run migrations");
        let store = SqlSessionStore::new(Arc::new(db.clone()));
        (db, store)
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
        store: &SqlSessionStore,
        session_id: Uuid,
        lifecycle: TransferState,
    ) {
        let files = vec![
            file_info(0, "hello.txt", 12),
            file_info(1, "docs/readme.md", 8),
        ];
        store
            .create_session(CreateSessionInput {
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
            })
            .await
            .expect("create receive session");
        mark_files_completed(store, session_id, &files).await;
    }

    #[tokio::test]
    async fn completed_receive_should_create_inbox_item_idempotently() {
        let (_db, store) = make_env().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&store, session_id, completed_state()).await;

        let first = store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .expect("create inbox item")
            .expect("inbox item");
        let second = store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .expect("create inbox item again")
            .expect("same inbox item");

        assert_eq!(first.item.id, second.item.id);
        assert_eq!(first.item.transfer_session_id, Some(session_id));
        assert_eq!(first.item.content_kind, InboxContentKind::Files);
        assert_eq!(first.item.item_count, 2);
        assert_eq!(first.files.len(), 2);
        assert!(first.files.iter().all(|file| !file.missing));

        let list = store.list_inbox_items(false).await.expect("list inbox");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, first.item.id);
    }

    #[tokio::test]
    async fn inbox_item_can_be_loaded_by_transfer_session_id() {
        let (_db, store) = make_env().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&store, session_id, completed_state()).await;
        let item = store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .unwrap()
            .unwrap();

        let queried = store
            .get_inbox_item_by_transfer_session_id(session_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(queried.item.id, item.item.id);
        assert_eq!(queried.item.transfer_session_id, Some(session_id));
        assert_eq!(queried.files.len(), 2);
    }

    #[tokio::test]
    async fn inbox_item_by_transfer_session_id_hides_deleted_records() {
        let (_db, store) = make_env().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&store, session_id, completed_state()).await;
        let item = store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .unwrap()
            .unwrap();

        store.delete_inbox_item_record(item.item.id).await.unwrap();

        let queried = store
            .get_inbox_item_by_transfer_session_id(session_id)
            .await
            .unwrap();
        assert!(queried.is_none());
    }

    #[tokio::test]
    async fn unfinished_receive_should_not_create_inbox_item() {
        let (_db, store) = make_env().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&store, session_id, TransferState::offered(0)).await;

        let item = store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .expect("ensure inbox item");
        assert!(item.is_none());
        assert!(store.list_inbox_items(false).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn clear_history_should_keep_inbox_records() {
        let (_db, store) = make_env().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&store, session_id, completed_state()).await;
        let item = store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .unwrap()
            .unwrap();

        store
            .clear_all_history()
            .await
            .expect("clear activity history");

        let list = store.list_inbox_items(false).await.expect("list inbox");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, item.item.id);
        assert!(
            store
                .get_transfer_projection(session_id)
                .await
                .expect("query projection")
                .is_none()
        );
    }

    /// D5：删单条传输记录只清活动账本——收件箱条目留下，外键被置空。
    ///
    /// 保证来自 `entity::inbox_item` 上声明的 `on_delete = "SetNull"`
    /// （schema builder 据此建外键）。
    /// 两条断言缺一不可：只断言「行还在」的话，外键写成 `RESTRICT`（根本删不掉）或
    /// FK 未生效（留一个指向已删会话的悬垂 id）都照样绿。
    #[tokio::test]
    async fn delete_session_should_keep_inbox_record_and_null_the_link() {
        let (db, store) = make_env().await;
        let session_id = Uuid::new_v4();
        create_receive_session(&store, session_id, completed_state()).await;
        let item = store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .unwrap()
            .unwrap();

        store
            .delete_session(session_id)
            .await
            .expect("delete transfer session");

        // 外键置空是行级事实，端口 DTO 看不到，故这条断言直查行。
        let row = entity::InboxItem::find_by_id(item.item.id)
            .one(&db)
            .await
            .expect("query inbox row")
            .expect("收件箱条目不应随传输记录消失");
        assert!(
            row.transfer_session_id.is_none(),
            "外键应被置空，而非留一个指向已删会话的悬垂 id"
        );
        assert_eq!(store.list_inbox_items(false).await.unwrap().len(), 1);
        assert!(
            store
                .get_transfer_projection(session_id)
                .await
                .expect("query projection")
                .is_none(),
            "活动账本这一侧应已删干净"
        );
    }

    /// 归属校验（D4）：`file_id` 全局唯一不代表它属于传进来的条目。
    #[tokio::test]
    async fn mark_file_missing_rejects_file_from_another_item() {
        let (_db, store) = make_env().await;
        let mine = make_inbox_item(&store, "Zoe", &[file_info(0, "我的.pdf", 5)]).await;
        let other = make_inbox_item(&store, "Zoe", &[file_info(1, "别人的.pdf", 5)]).await;

        let other_file_id = store
            .get_inbox_item_detail(other)
            .await
            .unwrap()
            .unwrap()
            .files[0]
            .id;

        assert!(
            store
                .mark_inbox_item_file_missing(mine, other_file_id, true)
                .await
                .is_err(),
            "不属于该条目的 file_id 必须报错，不能悄悄改到别人的文件上"
        );

        let my_file_id = store
            .get_inbox_item_detail(mine)
            .await
            .unwrap()
            .unwrap()
            .files[0]
            .id;
        store
            .mark_inbox_item_file_missing(mine, my_file_id, true)
            .await
            .expect("自己的文件应可标记");
        let detail = store.get_inbox_item_detail(mine).await.unwrap().unwrap();
        assert!(detail.files[0].missing);
        assert!(detail.item.missing, "条目级 missing 由文件行聚合");
    }

    /// 创建一个已完成接收会话并落库为收件箱条目，返回 item id。
    async fn make_inbox_item(store: &SqlSessionStore, peer_name: &str, files: &[FileInfo]) -> Uuid {
        let session_id = Uuid::new_v4();
        let total: u64 = files.iter().map(|file| file.size).sum();
        store
            .create_session(CreateSessionInput {
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
                lifecycle: completed_state(),
                policy: None,
                origin: None,
            })
            .await
            .expect("create receive session");
        mark_files_completed(store, session_id, files).await;
        store
            .ensure_inbox_item_for_completed_receive_session(session_id)
            .await
            .expect("ensure inbox item")
            .expect("inbox item")
            .item
            .id
    }

    /// 建条目时 `received_at` 一律写成会话完成时刻，排序测试要的时序只能事后校准
    /// ——没有对应的端口方法，故直改行。
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
    async fn search_finds_item_by_file_name_and_source_after_insert() {
        let (_db, store) = make_env().await;
        let id = make_inbox_item(
            &store,
            "Alice 的工作站",
            &[file_info(0, "季度合同扫描.pdf", 12)],
        )
        .await;

        let by_file_name = store.search_inbox("扫描", 10, false).await.unwrap();
        assert!(by_file_name.iter().any(|h| h.id == id), "文件名应命中");

        let by_source = store.search_inbox("Alice", 10, false).await.unwrap();
        assert!(by_source.iter().any(|h| h.id == id), "来源设备名应命中");

        let hit = by_file_name.iter().find(|h| h.id == id).unwrap();
        assert_eq!(hit.files.len(), 1);
        assert_eq!(hit.files[0].relative_path, "季度合同扫描.pdf");
        // **命中来自 `files_text`，不是标题**——索引里已经没有 title 列了（见
        // `title_is_not_indexed`）。但片段的**归属判断**仍看标题：条目行上显示的
        // 就是这个首文件名，命中它再给一条内容相同的片段只是把同一句话说两遍。
        // 片段判据的正面覆盖在 `swarmdrop-transfer` 的 `snippet_only_for_file_hits`
        // （领域层单测），这里只管「搜不搜得到」。
        assert!(hit.snippet.is_none(), "命中标题时不该产出片段");
    }

    /// 只命中 `relative_path` 时**要**产出片段——与上一个测试的否定断言凑成一对。
    ///
    /// 单有否定断言不够：`build_search_hit` 里唯一能产出 `Some` 的分支就是文件文本命中，
    /// 而「`files` 构造错了导致片段恒 `None`」在 SQL 端会一路绿到线上。Web 侧的对应测试是
    /// `crates/web/src/inbox.rs` 的 `invoices` 用例，两端对同一判据一正一反才算对齐。
    #[tokio::test]
    async fn search_snippet_present_when_only_file_path_matches() {
        let (_db, store) = make_env().await;
        let id = make_inbox_item(&store, "Dave", &[file_info(0, "invoices/c.pdf", 12)]).await;

        let hits = store.search_inbox("invoices", 10, false).await.unwrap();
        let hit = hits.iter().find(|h| h.id == id).expect("路径词应命中");
        // 「invoices」不在标题（单文件条目的标题是文件名 `c.pdf`）也不在来源名（Dave）里，
        // 所以这是该给片段的那种命中：用户要找的东西不在条目行上直接可见。
        assert!(
            hit.snippet
                .as_deref()
                .is_some_and(|s| s.contains("invoices")),
            "只命中文件路径时要带可读片段，实际: {:?}",
            hit.snippet
        );
    }

    #[tokio::test]
    async fn search_cjk_two_char_word_matches() {
        let (_db, store) = make_env().await;
        let id = make_inbox_item(&store, "Bob", &[file_info(0, "合同.pdf", 8)]).await;
        // 招牌回归点：2 字中文词在 trigram MATCH 下会返回空，必须靠 LIKE 兜底命中。
        let hits = store.search_inbox("合同", 10, false).await.unwrap();
        assert!(
            hits.iter().any(|h| h.id == id),
            "2 字中文词 '合同' 必须命中，不能返回空"
        );
    }

    #[tokio::test]
    async fn search_excludes_deleted_and_archived_by_default() {
        let (_db, store) = make_env().await;
        let kept = make_inbox_item(&store, "Carol", &[file_info(0, "报告.pdf", 5)]).await;
        let deleted = make_inbox_item(&store, "Carol", &[file_info(1, "报告草稿.pdf", 5)]).await;
        let archived = make_inbox_item(&store, "Carol", &[file_info(2, "报告归档.pdf", 5)]).await;

        store.delete_inbox_item_record(deleted).await.unwrap();
        store.archive_inbox_item(archived, true).await.unwrap();

        let default_ids: Vec<Uuid> = store
            .search_inbox("报告", 10, false)
            .await
            .unwrap()
            .iter()
            .map(|h| h.id)
            .collect();
        assert!(default_ids.contains(&kept));
        assert!(!default_ids.contains(&deleted), "软删条目不应返回");
        assert!(!default_ids.contains(&archived), "默认不返回已归档条目");

        let with_archived_ids: Vec<Uuid> = store
            .search_inbox("报告", 10, true)
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
        let (db, store) = make_env().await;
        let a = make_inbox_item(&store, "Dave", &[file_info(0, "票据A.pdf", 5)]).await;
        let b = make_inbox_item(&store, "Dave", &[file_info(1, "票据B.pdf", 5)]).await;
        let c = make_inbox_item(&store, "Dave", &[file_info(2, "票据C.pdf", 5)]).await;
        set_received_at(&db, a, 100).await;
        set_received_at(&db, b, 200).await;
        set_received_at(&db, c, 300).await;

        let hits = store.search_inbox("票据", 2, false).await.unwrap();
        assert_eq!(hits.len(), 2, "limit 应截断到 2");
        assert_eq!(hits[0].id, c, "最新（received_at 最大）排最前");
        assert_eq!(hits[1].id, b);
    }

    #[tokio::test]
    async fn search_empty_query_and_no_match_return_empty() {
        let (_db, store) = make_env().await;
        make_inbox_item(&store, "Erin", &[file_info(0, "发票.pdf", 5)]).await;
        assert!(store.search_inbox("", 10, false).await.unwrap().is_empty());
        assert!(
            store
                .search_inbox("   ", 10, false)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .search_inbox("不存在的关键词zzz", 10, false)
                .await
                .unwrap()
                .is_empty()
        );
    }

    /// 条目行 `title` 的哨兵值：**它不在任何索引列里**，所以拿它当查询词必须搜不到。
    /// `title_is_not_indexed` 靠这一点把「标题不参与检索」钉成可执行断言。
    const TITLE_SENTINEL: &str = "哨兵标题不该被检索到";

    /// 按语料直接造一条收件箱条目 + 对应的索引行，返回 item id。
    ///
    /// 不走 `ensure_*`：那条路径的 files_text 由文件行派生，摆不出语料要的三列
    /// 任意组合（尤其 `extracted_text`——SQL 侧目前没有任何写入它的生产路径）。
    ///
    /// 条目行的 `title` 一律填 [`TITLE_SENTINEL`]：它是展示字段，不进 `inbox_search_index`，
    /// 语料里因此没有它的位置。
    async fn insert_indexed_item(
        db: &DatabaseConnection,
        source_name: &str,
        files_text: &str,
        extracted_text: Option<&str>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        entity::inbox_item::ActiveModel::builder()
            .set_id(id)
            .set_transfer_session_id(None)
            .set_source_peer_id(entity::PeerId("peer-corpus".to_string()))
            .set_source_name(source_name.to_string())
            .set_source_kind(entity::InboxSourceKind::PairedDevice)
            .set_content_kind(InboxContentKind::Files)
            .set_title(TITLE_SENTINEL.to_string())
            .set_item_count(1)
            .set_total_size(1)
            .set_root_path(None)
            .set_content_hash(None)
            .set_received_at(now_ms())
            .insert(db)
            .await
            .expect("insert inbox item");
        entity::inbox_search_index::ActiveModel::builder()
            .set_item_id(id)
            .set_source_name(source_name.to_string())
            .set_files_text(files_text.to_string())
            // 语料的 `None`（「这一端没有文本抽取」）在 SQL 侧落成空串：唯一的生产写入
            // 路径（`ensure_*`）对没有抽取结果的条目写的就是 ''，库里不存在 NULL 的
            // extracted_text，列本身也是 NOT NULL。
            .set_extracted_text(extracted_text.unwrap_or_default().to_string())
            .insert(db)
            .await
            .expect("insert search index row");
        id
    }

    /// 跨端一致性（conformance）：同一批 [`INBOX_MATCH_CASES`] 灌进真实 SQLite 走
    /// `search_inbox`，断言与 [`swarmdrop_transfer::inbox::inbox_matches`] 同一批 `expected`。
    ///
    /// 「SQL 的 `LIKE` 是 `inbox_matches` 的复刻」此前只是一句注释——SQL 多匹配了
    /// `extracted_text` 一列而那个函数根本没有这个入参，规范与实现从一开始就是错位的。
    /// 这条测试是那句注释的可执行形式：任一端改判据（少接一列、忘了 `escape_like`、
    /// 换回 FTS5 `MATCH`）都会在这里变红。
    #[tokio::test]
    async fn sql_like_matches_shared_corpus_same_as_inbox_matches() {
        use swarmdrop_transfer::inbox::INBOX_MATCH_CASES;

        let (db, store) = make_env().await;

        // 一条语料一条条目，全部灌进同一个库：断言只看「自己那条在不在结果里」。
        // 别的条目顺带命中同一个词无所谓——真实库里本来就有别的条目，判据是逐条目的。
        let mut ids = Vec::with_capacity(INBOX_MATCH_CASES.len());
        for case in INBOX_MATCH_CASES {
            ids.push(
                insert_indexed_item(&db, case.source_name, case.files_text, case.extracted_text)
                    .await,
            );
        }

        for (case, id) in INBOX_MATCH_CASES.iter().zip(ids) {
            // limit 取得远大于语料条数，避免命中被截断而误判成「不命中」。
            let matched = store
                .search_inbox(case.query, 1000, false)
                .await
                .expect("search inbox")
                .iter()
                .any(|hit| hit.id == id);
            assert_eq!(
                matched, case.expected,
                "SQL 的 LIKE 与 inbox_matches 分叉: {}（query={:?}）",
                case.name, case.query
            );
        }
    }

    /// 条目标题**不是索引列**：拿标题原文当查询词必须一条都搜不到。
    ///
    /// 守的是「删掉 `inbox_search_index.title` 之后没人把它加回来」。加回来不会让任何
    /// 现有断言变红——那一列存的是首文件名，而首文件名已经在 `files_text` 里，
    /// 多一列只是把同一个词再匹配一遍。用一个**只存在于 `inbox_items.title`** 的哨兵词
    /// 才能把这件事测出来。
    #[tokio::test]
    async fn title_is_not_indexed() {
        let (db, store) = make_env().await;
        let id = insert_indexed_item(&db, "Alice", "a.pdf a.pdf", None).await;

        let hits = store
            .search_inbox(TITLE_SENTINEL, 1000, false)
            .await
            .expect("search inbox");
        assert!(
            !hits.iter().any(|hit| hit.id == id),
            "标题不该参与检索，却命中了：{TITLE_SENTINEL}"
        );

        // 反向对照：同一条条目按文件名仍然搜得到，证明它确实进了索引、
        // 上面那条不命中不是因为条目本身没被索引。
        assert!(
            store
                .search_inbox("a.pdf", 1000, false)
                .await
                .expect("search inbox")
                .iter()
                .any(|hit| hit.id == id),
            "文件名应当仍能命中"
        );
    }

    #[tokio::test]
    async fn search_works_with_empty_extracted_text() {
        let (_db, store) = make_env().await;
        let id = make_inbox_item(&store, "Grace", &[file_info(0, "笔记.txt", 5)]).await;
        // 所有 extracted_text 为空：仅靠标题/来源名/文件名仍能命中，且无关词不凭空命中。
        assert!(
            store
                .search_inbox("笔记", 10, false)
                .await
                .unwrap()
                .iter()
                .any(|h| h.id == id)
        );
        assert!(
            store
                .search_inbox("OCR内容xyz", 10, false)
                .await
                .unwrap()
                .is_empty(),
            "extracted_text 为空，不应凭空命中"
        );
    }

    /// `repair_*` 是修复「`ensure_*` 当时写失败」的通道，不是存储格式迁移：
    /// 已完成接收会话缺条目时补出来，已有条目的会话不重复补。
    #[tokio::test]
    async fn repair_backfills_only_missing_items() {
        let (_db, store) = make_env().await;
        let with_item = Uuid::new_v4();
        create_receive_session(&store, with_item, completed_state()).await;
        store
            .ensure_inbox_item_for_completed_receive_session(with_item)
            .await
            .unwrap()
            .unwrap();

        let without_item = Uuid::new_v4();
        create_receive_session(&store, without_item, completed_state()).await;
        // 未完成接收不参与补建。
        let unfinished = Uuid::new_v4();
        create_receive_session(&store, unfinished, TransferState::offered(0)).await;

        let repaired = store
            .repair_missing_inbox_items_for_completed_receives()
            .await
            .expect("repair");
        assert_eq!(repaired.len(), 1);
        assert_eq!(repaired[0].item.transfer_session_id, Some(without_item));
        assert_eq!(store.list_inbox_items(false).await.unwrap().len(), 2);
    }
}
