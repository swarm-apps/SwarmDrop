//! 收件箱表（[`InboxStore`](swarmdrop_transfer::store::InboxStore) 的 Web 后端）：
//! **内存读缓存 + IndexedDB 写穿**，与 [`crate::store`] 的会话表同一套路子。
//!
//! 收件箱在 Web 端曾经不是一张表，而是「`direction=receive` 且 `terminalReason=completed`
//! 的会话投影」。那个做法有一处结构性缺陷：会话表有 `HISTORY_CAP` 淘汰，而收件箱是
//! **结果账本**，不该随过程账本一起被挤掉。建真表之后，「清空传输历史不动收件箱」
//! 这条三端共有的不变量在浏览器上才真正成立。
//!
//! [`WebInboxTable`] 是**自包含**的：自己的内存 map、自己的 IndexedDB 读写，不认识会话表。
//! 唯一需要跨界的是 [`WebInboxTable::ensure_from_session`]——它要读会话行与文件行，
//! 于是那两样**作为参数传进来**，依赖方向单向（会话表 → 收件箱表，反向没有边）。
//! 同理，`InboxItemDetail.transfer` 这一格由 `WebTransferStore` 在委托返回后补，
//! 本模块一律留 `None`。
//!
//! 条目的标题 / 内容指纹 / 聚合文本 / 来源分类 / 命中判据 / 片段全部调
//! [`swarmdrop_transfer::inbox`] 的共享规则，**Web 侧一行都不重写**——分叉了同一批文件
//! 在浏览器与桌面就会得到不同的标题与不同的内容指纹，而后者是跨端去重的唯一判据。
//!
//! ## 存量数据：不迁移、不回填、不双写（本 change 的 design D7）
//!
//! `DB_VERSION` 提到 4 只为让 `onupgradeneeded` 建出 `inbox` store。**老库里已经存在的
//! 「终态 + Completed 接收会话」不会被自动补出收件箱条目**：落地后第一次打开 Web 应用，
//! 收件箱是空的而传输历史是满的——那是预期结果，不是 bug。
//!
//! 理由是 Web 端目前没有真实用户，「保住旧数据」的收益是零，而回填 / 双写 / 兼容层
//! 不表达任何业务规则，只表达「我们曾经用过另一种存法」。
//!
//! 要与 `repair_missing_inbox_items_for_completed_receives` 区分开：那是**显式调用**的
//! 修复通道（修「`ensure_*` 当时写失败」这个长期存在的洞），不是迁移——它照样实现，
//! 但**没有任何代码在启动路径上调用它**。

use std::collections::HashMap;
use std::sync::Mutex;

use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use swarmdrop_host::{AppError, AppResult, CoreSaveLocation};
use swarmdrop_transfer::inbox::{
    InboxFileFacts, InboxHitFile, InboxItemContent, InboxItemDetail, InboxItemFileEntry,
    InboxItemSummary, InboxSearchHit, inbox_content_hash, inbox_files_text, inbox_matches,
    inbox_primary_file_name, inbox_snippet, inbox_source_kind, is_completed_receive,
};
use swarmdrop_transfer::store::content_root_of;
use swarmdrop_transfer::text_delivery::{TextDeliveryRecord, text_preview, validate_text_body};
use uuid::Uuid;

use crate::idb;

/// 一条收件箱条目的内存记录：条目行 + 其文件行。
struct StoredInboxItem {
    item: entity::inbox_item::Model,
    files: Vec<entity::inbox_item_file::Model>,
    /// 接收文本与 Inbox 投影同存一条 IndexedDB 记录，提交不可分割。
    text_delivery: Option<TextDeliveryRecord>,
}

/// 内存主表 + 按传输会话反查的索引。
///
/// 两者同锁同生命周期、**只由 [`Self::insert`] 一处写入**——分开维护就会留下
/// 「加条目忘了加索引」的口子，而症状不是报错：`has_item_for_session` 返回 false，
/// `repair_*` 于是反复给同一个会话补建条目。
#[derive(Default)]
struct InboxTables {
    items: HashMap<Uuid, StoredInboxItem>,
    /// `transfer_session_id → item id`。**不随软删移除**——查重刻意不排除软删项
    /// （否则用户删过一次的条目会被反复补回来），索引的可见性判断留给读取方。
    by_session: HashMap<Uuid, Uuid>,
    by_text_delivery: HashMap<Uuid, Uuid>,
}

impl InboxTables {
    /// 插入条目的**唯一入口**：主表与索引在同一处更新。
    fn insert(&mut self, stored: StoredInboxItem) {
        if let Some(session_id) = stored.item.transfer_session_id {
            self.by_session.insert(session_id, stored.item.id);
        }
        if let Some(delivery_id) = stored.item.text_delivery_id {
            self.by_text_delivery.insert(delivery_id, stored.item.id);
        }
        self.items.insert(stored.item.id, stored);
    }

    /// 按传输会话取条目（O(1)）。`exclude_deleted=false` 时连软删项一起返回。
    fn by_session(&self, session_id: Uuid, exclude_deleted: bool) -> Option<&StoredInboxItem> {
        self.items
            .get(self.by_session.get(&session_id)?)
            .filter(|stored| !exclude_deleted || stored.item.deleted_at.is_none())
    }

    fn by_text_delivery(
        &self,
        delivery_id: Uuid,
        exclude_deleted: bool,
    ) -> Option<&StoredInboxItem> {
        self.items
            .get(self.by_text_delivery.get(&delivery_id)?)
            .filter(|stored| !exclude_deleted || stored.item.deleted_at.is_none())
    }
}

/// IndexedDB 写穿的收件箱表。
#[derive(Default)]
pub struct WebInboxTable {
    tables: Mutex<InboxTables>,
}

impl WebInboxTable {
    /// 从 IndexedDB 恢复。单条记录损坏只跳过它（warn）而非让整个节点起不来；
    /// 读不到库（隐私模式 / 配额拒绝）时退化成纯内存表，节点照常可用。
    pub async fn load() -> Self {
        let table = Self::default();
        let records = match SendWrapper::new(idb::get_all(idb::INBOX_STORE)).await {
            Ok(records) => records,
            Err(e) => {
                tracing::warn!("读取收件箱持久化失败，本次以内存态运行: {e:?}");
                return table;
            }
        };

        let mut tables = table.tables.lock().unwrap();
        for record in records {
            let Some(json) = record.as_string() else {
                tracing::warn!("跳过一条非字符串的收件箱记录");
                continue;
            };
            let persisted: PersistedInboxItem = match serde_json::from_str(&json) {
                Ok(value) => value,
                Err(e) => {
                    tracing::warn!("跳过一条无法解析的收件箱记录: {e}");
                    continue;
                }
            };
            tables.insert(StoredInboxItem::from(persisted));
        }
        drop(tables);
        table
    }

    /// 该传输会话是否已有收件箱条目（`repair_*` 的判据；与幂等创建一样**不排除软删项**，
    /// 否则删过一次的条目会被反复补建出来）。
    ///
    /// 走 `by_session` 索引：`repair_*` 会对每条已完成接收会话问一次，线性扫描时它与
    /// 紧随其后的 `ensure_from_session`（同样查一次重）合起来是 O(N·M) 的两遍全表。
    pub fn has_item_for_session(&self, session_id: Uuid) -> bool {
        self.tables
            .lock()
            .unwrap()
            .by_session
            .contains_key(&session_id)
    }

    /// 从已完成接收会话幂等创建收件箱条目。
    ///
    /// 会话行与文件行由调用方（`WebTransferStore`）传入——本表不认识会话表（模块注释）。
    /// 非 receive、未完成、失败/暂停/取消的会话返回 `Ok(None)`，**这不是错误**。
    pub async fn ensure_from_session(
        &self,
        session: &entity::transfer_session::Model,
        files: &[entity::transfer_file::Model],
    ) -> AppResult<Option<InboxItemDetail>> {
        if let Some(existing) = self.detail_by_session_id(session.session_id, false) {
            return Ok(Some(existing));
        }

        // 「哪些会话进收件箱」是跨端共享的判据（SQL 侧的 `ensure_*` 与 Web 侧的补建扫描
        // 都必须同义），故调共享函数而不在这里重写那三段合取。
        if !is_completed_receive(session) {
            return Ok(None);
        }

        // 已完成接收必有保存位置（不变量）；缺失是数据异常，显式报错。
        if session.save_path.is_none() {
            return Err(AppError::Transfer(
                "已完成接收会话缺少保存位置，无法创建收件箱条目".into(),
            ));
        }

        let item_count = i32::try_from(files.len())
            .map_err(|_| AppError::Transfer("收件箱文件数量超出可表示范围".into()))?;
        // 顺序契约：领域规则要求按协议层的 `file_id` 升序（见 `InboxFileFacts` 的文档）。
        // **这里排序不是可选项**——SQL 侧同样按 `file_id` 排，两端一旦分叉，同一批文件
        // 就会得到不同的 `content_hash`，而那是跨端去重的唯一判据。
        // 传入的 `files` 顺序是对端 manifest 的原始顺序，`file_id` 由对端填，不保证有序。
        let mut ordered: Vec<&entity::transfer_file::Model> = files.iter().collect();
        ordered.sort_by_key(|file| file.file_id);
        let files = &ordered[..];
        // 标题 / 内容指纹两条都是跨端共享的领域规则，一律调 swarmdrop_transfer::inbox
        // 的那一份，本文件只负责把行类型摊成中立视图。
        let facts: Vec<InboxFileFacts<'_>> = files.iter().map(|file| file_facts(file)).collect();
        let title = inbox_primary_file_name(&facts);
        let content_hash = inbox_content_hash(&facts);
        // root_path = 真实容器目录（与传输投影 content_root 同一处解析：缺 local_dir 时
        // 回退存储根）。兜底收口在 content_root_of 一处，不再重复。
        let save_location = session.save_path.clone().map(CoreSaveLocation::from);
        let root_path = content_root_of(
            files.iter().map(|file| file.local_dir.as_deref()),
            save_location.as_ref(),
        );
        let source_kind = inbox_source_kind(session.origin.as_deref());

        let inbox_id = Uuid::new_v4();
        let item = entity::inbox_item::Model {
            id: inbox_id,
            transfer_session_id: Some(session.session_id),
            text_delivery_id: None,
            source_peer_id: session.peer_id.clone(),
            source_name: session.peer_name.clone(),
            source_kind,
            content_kind: entity::InboxContentKind::Files,
            title,
            item_count,
            total_size: session.total_size,
            root_path,
            content_hash: Some(content_hash),
            received_at: session.finished_at.unwrap_or_else(now_ms),
            last_opened_at: None,
            archived_at: None,
            deleted_at: None,
        };

        let mut item_files = Vec::with_capacity(files.len());
        for (index, file) in files.iter().enumerate() {
            // 落盘位置是唯一事实源（OPFS 句柄路径无法由「目录 + 相对路径」拼接推导）。
            // 已完成接收的文件必然写过它——缺失即数据异常，显式报错不做推导。
            let Some(local_path) = file.local_path.clone() else {
                return Err(AppError::Transfer(format!(
                    "已完成接收文件缺少落盘路径记录: {}（旧版本数据，请清除应用数据后重试）",
                    file.name
                )));
            };
            item_files.push(entity::inbox_item_file::Model {
                // 浏览器没有自增主键：用**条目内序号**（写入时定、之后不变）。全局不唯一，
                // 但端口的 `mark_inbox_item_file_missing` 吃 `(item_id, file_id)` 双参数，
                // 定位不依赖全局唯一性（design D9）。
                id: index as i32,
                inbox_item_id: inbox_id,
                transfer_file_id: Some(file.file_id),
                relative_path: file.relative_path.clone(),
                name: file.name.clone(),
                size: file.size,
                checksum: file.checksum.clone(),
                local_path,
                missing: false,
            });
        }

        self.tables.lock().unwrap().insert(StoredInboxItem {
            item,
            files: item_files,
            text_delivery: None,
        });
        self.persist(inbox_id).await?;
        Ok(self.detail(inbox_id))
    }

    /// 列出收件箱条目：排除软删项，按 `received_at` 倒序；`include_archived=false` 时排除已归档项。
    ///
    /// 产**详情**而不是 summary（`transfer` 那格照旧留 `None`，由 `WebTransferStore` 补）：
    /// 浏览器侧的收件箱是全内存表，列表与详情读的是同一份数据，摘掉文件行只会逼调用方
    /// 逐条再要一次（wasm 边界上就是 1 + N 次调用与 N 份重复克隆）。要 summary 的调用方
    /// 自取 `detail.item` 即可。
    pub fn list(&self, include_archived: bool) -> Vec<InboxItemDetail> {
        let tables = self.tables.lock().unwrap();
        let mut items: Vec<(i64, InboxItemDetail)> = tables
            .items
            .values()
            .filter(|stored| visible(stored, include_archived))
            .map(|stored| (stored.item.received_at, detail_of(stored)))
            .collect();
        sort_by_received_at_desc(&mut items);
        items.into_iter().map(|(_, detail)| detail).collect()
    }

    /// 接收文本账本与 Inbox 投影使用同一 IndexedDB 行，因此写入只有全成或全败。
    pub async fn persist_text_delivery(
        &self,
        record: TextDeliveryRecord,
    ) -> AppResult<InboxItemDetail> {
        validate_text_body(&record.body)?;
        let delivery_id = record.delivery_id;
        if let Some(existing) = self.text_delivery(delivery_id) {
            if existing.peer_id != record.peer_id || existing.body != record.body {
                return Err(AppError::Transfer("文本投递标识与既有内容冲突".into()));
            }
            return self
                .detail_by_text_delivery_id(delivery_id, false)
                .ok_or_else(|| AppError::Transfer("既有文本投递缺少收件箱投影".into()));
        }

        let body_size = i64::try_from(record.body.len())
            .map_err(|_| AppError::Transfer("文本长度超出可表示范围".into()))?;
        let item = entity::inbox_item::Model {
            id: Uuid::new_v4(),
            transfer_session_id: None,
            text_delivery_id: Some(delivery_id),
            source_peer_id: entity::PeerId(record.peer_id.clone()),
            source_name: record.peer_name.clone(),
            source_kind: entity::InboxSourceKind::PairedDevice,
            content_kind: entity::InboxContentKind::Text,
            // 与 SQL 端保持同一条不变量：列表读取摘要时即可展示正文预览。
            title: text_preview(&record.body),
            item_count: 1,
            total_size: body_size,
            root_path: None,
            content_hash: None,
            received_at: record.updated_at,
            last_opened_at: None,
            archived_at: None,
            deleted_at: None,
        };
        let item_id = item.id;
        self.tables.lock().unwrap().insert(StoredInboxItem {
            item,
            files: Vec::new(),
            text_delivery: Some(record),
        });
        if let Err(error) = self.persist(item_id).await {
            // IndexedDB 是这条投递的提交点。写穿失败时撤销内存变更，避免同一进程把未落库
            // 的正文误报给发送端为已送达。
            self.tables.lock().unwrap().items.remove(&item_id);
            self.tables
                .lock()
                .unwrap()
                .by_text_delivery
                .remove(&delivery_id);
            return Err(error);
        }
        self.detail(item_id)
            .ok_or_else(|| AppError::Transfer("文本收件箱投影创建后不可读取".into()))
    }

    /// 从接收方向的嵌入式账本读取记录。
    pub fn text_delivery(&self, delivery_id: Uuid) -> Option<TextDeliveryRecord> {
        self.tables
            .lock()
            .unwrap()
            .by_text_delivery(delivery_id, false)
            .and_then(|stored| stored.text_delivery.clone())
    }

    fn detail_by_text_delivery_id(
        &self,
        delivery_id: Uuid,
        exclude_deleted: bool,
    ) -> Option<InboxItemDetail> {
        self.tables
            .lock()
            .unwrap()
            .by_text_delivery(delivery_id, exclude_deleted)
            .map(detail_of)
    }

    /// 子串检索：浏览器没有 FTS，直接对内存里的每条条目跑
    /// [`inbox_matches`]——它是命中判据的规范定义，SQL 侧那段 `LIKE ... ESCAPE '\'`
    /// 复刻的正是它。命中集合、排序（`received_at` 倒序）、片段三样与 SQL 侧逐条对齐。
    ///
    /// 规模合理性：条目数与用户实际接收次数同阶（几十到几百），线性扫描的成本远低于
    /// 一次 IndexedDB 往返。
    pub fn search(&self, query: &str, limit: usize, include_archived: bool) -> Vec<InboxSearchHit> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        let tables = self.tables.lock().unwrap();
        let mut hits: Vec<(i64, InboxSearchHit)> = tables
            .items
            .values()
            .filter(|stored| visible(stored, include_archived))
            .filter_map(|stored| {
                let facts: Vec<InboxFileFacts<'_>> = stored.files.iter().map(item_facts).collect();
                let files_text = inbox_files_text(&facts);
                if !inbox_matches(
                    trimmed,
                    &stored.item.source_name,
                    &files_text,
                    // 抽取正文恒 `None`：浏览器没有文本抽取能力。**不是空串**——空串意为
                    // 「抽过、没抽到东西」，而这里是「这一端根本没有这个索引列」。
                    None,
                ) {
                    return None;
                }
                let files = hit_files(stored);
                let snippet = inbox_snippet(
                    trimmed,
                    &stored.item.title,
                    &stored.item.source_name,
                    &files,
                );
                Some((
                    stored.item.received_at,
                    InboxSearchHit {
                        id: stored.item.id,
                        title: stored.item.title.clone(),
                        source_name: stored.item.source_name.clone(),
                        item_count: stored.item.item_count,
                        root_path: stored.item.root_path.clone(),
                        received_at: stored.item.received_at,
                        snippet,
                        files,
                    },
                ))
            })
            .collect();
        sort_by_received_at_desc(&mut hits);
        hits.truncate(limit);
        hits.into_iter().map(|(_, hit)| hit).collect()
    }

    /// 加载收件箱详情（`transfer` 恒为 `None`，由 `WebTransferStore` 补）。软删项不可见。
    pub fn detail(&self, item_id: Uuid) -> Option<InboxItemDetail> {
        let tables = self.tables.lock().unwrap();
        tables
            .items
            .get(&item_id)
            .filter(|stored| stored.item.deleted_at.is_none())
            .map(detail_of)
    }

    /// 加载与指定传输会话关联的可见收件箱详情（同样排除软删项）。
    pub fn detail_by_session(&self, session_id: Uuid) -> Option<InboxItemDetail> {
        self.detail_by_session_id(session_id, true)
    }

    /// 标记条目最近打开时间。条目不存在时静默成功。
    pub async fn mark_opened(&self, item_id: Uuid) -> AppResult<()> {
        let now = now_ms();
        self.mutate_and_persist(item_id, |stored| stored.item.last_opened_at = Some(now))
            .await
    }

    /// 归档或取消归档。条目不存在时静默成功。
    pub async fn archive(&self, item_id: Uuid, archived: bool) -> AppResult<()> {
        let stamp = archived.then(now_ms);
        self.mutate_and_persist(item_id, |stored| stored.item.archived_at = stamp)
            .await
    }

    /// 软删除记录（行仍在，只打时间戳）；是否删除本地文件由宿主在调用前完成。
    /// 条目不存在时静默成功。
    pub async fn delete_record(&self, item_id: Uuid) -> AppResult<()> {
        let now = now_ms();
        self.mutate_and_persist(item_id, |stored| {
            stored.item.deleted_at = Some(now);
            // 文本正文属于收件箱记录本身；软删仍需清掉其本地副本。
            stored.text_delivery = None;
        })
        .await
    }

    /// 标记条目内某个文件的缺失状态。
    ///
    /// 归属校验是端口的义务（见 `InboxStore` 的方法文档）：`file_id` 在 Web 侧只是条目内
    /// 序号，不校验的话它在别的条目下同样存在，一路改到别人的文件上。
    pub async fn mark_file_missing(
        &self,
        item_id: Uuid,
        file_id: i32,
        missing: bool,
    ) -> AppResult<()> {
        {
            let mut tables = self.tables.lock().unwrap();
            let stored = tables
                .items
                .get_mut(&item_id)
                .ok_or_else(|| AppError::SessionNotFound("收件箱条目不存在".into()))?;
            let Some(file) = stored.files.iter_mut().find(|file| file.id == file_id) else {
                return Err(AppError::Transfer("收件箱文件不属于该条目".into()));
            };
            file.missing = missing;
        }
        self.persist(item_id).await
    }

    /// `exclude_deleted=false` 时不排除软删项（幂等创建的查重用——排除了的话删过一次的
    /// 条目会被反复补建出来），`true` 时排除（面向 UI/API 的查询用）。
    ///
    /// 走 `by_session` 索引，不再线性扫描（见 [`InboxTables::by_session`]）。
    fn detail_by_session_id(
        &self,
        session_id: Uuid,
        exclude_deleted: bool,
    ) -> Option<InboxItemDetail> {
        let tables = self.tables.lock().unwrap();
        tables
            .by_session(session_id, exclude_deleted)
            .map(detail_of)
    }

    /// 改一条已存在的条目并写穿；条目不存在时什么也不做（端口契约：静默成功）。
    async fn mutate_and_persist(
        &self,
        item_id: Uuid,
        apply: impl FnOnce(&mut StoredInboxItem),
    ) -> AppResult<()> {
        {
            let mut tables = self.tables.lock().unwrap();
            let Some(stored) = tables.items.get_mut(&item_id) else {
                return Ok(());
            };
            apply(stored);
        }
        self.persist(item_id).await
    }

    /// 把某条目的当前内存态写进 IndexedDB。条目已被移除时跳过。
    async fn persist(&self, item_id: Uuid) -> AppResult<()> {
        let Some(persisted) = self.snapshot(item_id) else {
            return Ok(());
        };
        let json = serde_json::to_string(&persisted)
            .map_err(|e| AppError::Transfer(format!("序列化收件箱条目失败: {e}")))?;
        SendWrapper::new(idb::put_string(
            idb::INBOX_STORE,
            &item_id.to_string(),
            &json,
        ))
        .await
        .map_err(AppError::from)
    }

    /// 取快照（持锁期间不 await）。
    fn snapshot(&self, item_id: Uuid) -> Option<PersistedInboxItem> {
        let tables = self.tables.lock().unwrap();
        tables.items.get(&item_id).map(PersistedInboxItem::from)
    }
}

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

/// 列表 / 检索的可见性：软删项一律不可见，归档项按开关。
fn visible(stored: &StoredInboxItem, include_archived: bool) -> bool {
    stored.item.deleted_at.is_none() && (include_archived || stored.item.archived_at.is_none())
}

/// `received_at` 倒序。`HashMap` 的迭代序不可依赖，而端口契约要求确定性顺序——
/// 没有它，「取最近 N 条」在浏览器上取到的就是随机 N 条。
fn sort_by_received_at_desc<T>(rows: &mut [(i64, T)]) {
    rows.sort_by_key(|(received_at, _)| std::cmp::Reverse(*received_at));
}

/// 收件箱文件行 → 收件箱规则的中立视图。
fn item_facts(file: &entity::inbox_item_file::Model) -> InboxFileFacts<'_> {
    InboxFileFacts {
        name: &file.name,
        relative_path: &file.relative_path,
        checksum: &file.checksum,
        size: file.size,
    }
}

/// 传输文件行 → 收件箱规则的中立视图（建条目时用）。
fn file_facts(file: &entity::transfer_file::Model) -> InboxFileFacts<'_> {
    InboxFileFacts {
        name: &file.name,
        relative_path: &file.relative_path,
        checksum: &file.checksum,
        size: file.size,
    }
}

fn hit_files(stored: &StoredInboxItem) -> Vec<InboxHitFile> {
    stored
        .files
        .iter()
        .map(|file| InboxHitFile {
            name: file.name.clone(),
            relative_path: file.relative_path.clone(),
        })
        .collect()
}

fn summary_of(stored: &StoredInboxItem) -> InboxItemSummary {
    let item = &stored.item;
    InboxItemSummary {
        id: item.id,
        transfer_session_id: item.transfer_session_id,
        text_delivery_id: item.text_delivery_id,
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
        // 条目级 missing 由文件行聚合（与 SQL 实现同义）。
        missing: stored.files.iter().any(|file| file.missing),
    }
}

/// `transfer` 留 `None`：会话投影由 `WebTransferStore` 在委托返回后补（模块注释）。
fn detail_of(stored: &StoredInboxItem) -> InboxItemDetail {
    InboxItemDetail {
        item: summary_of(stored),
        content: match &stored.text_delivery {
            Some(delivery) => InboxItemContent::Text {
                body: delivery.body.clone(),
            },
            None => InboxItemContent::Files {
                entries: stored
                    .files
                    .iter()
                    .map(|file| InboxItemFileEntry {
                        id: file.id,
                        transfer_file_id: file.transfer_file_id,
                        relative_path: file.relative_path.clone(),
                        name: file.name.clone(),
                        size: file.size,
                        checksum: file.checksum.clone(),
                        local_path: file.local_path.clone(),
                        missing: file.missing,
                    })
                    .collect(),
                transfer: None,
            },
        },
    }

    // ── 持久化 DTO ────────────────────────────────────────────────────────────────
    //
    // 与 `crate::store` 同一手法：entity 的 `Model` 没有 serde derive（`#[sea_orm::model]`
    // 不把用户 derive 转发给生成的 `Model`，只到 `ModelEx`），故落库形态用 serde 的
    // **remote derive** 在此显式声明。字段清单仍然逐个写出来（**这就是存储格式的声明点**），
    // 但省掉两个方向的手工搬运——那种写法漏一个字段编译照过、只在运行时静默丢数据，
    // 而 remote derive 在 entity 加列时直接编译不过。
}

#[derive(Serialize, Deserialize)]
struct PersistedInboxItem {
    #[serde(with = "InboxItemRowDef")]
    item: entity::inbox_item::Model,
    files: Vec<PersistedInboxFile>,
    text_delivery: Option<TextDeliveryRecord>,
}

/// newtype 只为给 `Vec` 里的元素挂上 `with`（serde 的 `with` 作用于字段，不能作用于泛型参数）。
#[derive(Serialize, Deserialize)]
struct PersistedInboxFile(#[serde(with = "InboxItemFileRowDef")] entity::inbox_item_file::Model);

#[derive(Serialize, Deserialize)]
#[serde(remote = "entity::inbox_item::Model")]
#[allow(dead_code, reason = "字段只被 serde 生成的 remote 读写，本地无读取方")]
struct InboxItemRowDef {
    id: Uuid,
    transfer_session_id: Option<Uuid>,
    text_delivery_id: Option<Uuid>,
    source_peer_id: entity::PeerId,
    source_name: String,
    source_kind: entity::InboxSourceKind,
    content_kind: entity::InboxContentKind,
    title: String,
    item_count: i32,
    total_size: i64,
    root_path: Option<String>,
    content_hash: Option<String>,
    received_at: i64,
    last_opened_at: Option<i64>,
    archived_at: Option<i64>,
    deleted_at: Option<i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "entity::inbox_item_file::Model")]
#[allow(dead_code, reason = "字段只被 serde 生成的 remote 读写，本地无读取方")]
struct InboxItemFileRowDef {
    id: i32,
    inbox_item_id: Uuid,
    transfer_file_id: Option<i32>,
    relative_path: String,
    name: String,
    size: i64,
    checksum: String,
    local_path: String,
    missing: bool,
}

impl From<&StoredInboxItem> for PersistedInboxItem {
    fn from(stored: &StoredInboxItem) -> Self {
        Self {
            item: stored.item.clone(),
            files: stored
                .files
                .iter()
                .cloned()
                .map(PersistedInboxFile)
                .collect(),
            text_delivery: stored.text_delivery.clone(),
        }
    }
}

impl From<PersistedInboxItem> for StoredInboxItem {
    fn from(persisted: PersistedInboxItem) -> Self {
        Self {
            item: persisted.item,
            files: persisted.files.into_iter().map(|file| file.0).collect(),
            text_delivery: persisted.text_delivery,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use wasm_bindgen_test::wasm_bindgen_test;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    const SAVE_ROOT: &str = "/inbox-table-test";

    /// 已完成接收的会话行：收件箱只认 `Receive` + `Terminal` + `Completed` 这一种组合。
    /// `finished_at` 直接决定条目的 `received_at`——排序相关的测试靠它排时序，
    /// 不依赖 `js_sys::Date::now()`。
    fn receive_session(
        session_id: Uuid,
        peer_name: &str,
        finished_at: i64,
    ) -> entity::transfer_session::Model {
        entity::transfer_session::Model {
            session_id,
            direction: entity::TransferDirection::Receive,
            peer_id: entity::PeerId("peer-inbox".into()),
            peer_name: peer_name.to_string(),
            total_size: 20,
            transferred_bytes: 20,
            status: entity::SessionStatus::Completed,
            phase: entity::TransferPhase::Terminal,
            suspended_reason: None,
            terminal_reason: Some(entity::TerminalReason::Completed),
            epoch: 0,
            recoverable: false,
            source_fingerprint: None,
            started_at: finished_at - 1_000,
            updated_at: finished_at,
            finished_at: Some(finished_at),
            error_message: None,
            policy_action: None,
            policy_reason: None,
            origin: None,
            save_path: Some(entity::SaveLocation::Path {
                path: SAVE_ROOT.to_string(),
            }),
        }
    }

    /// 已落盘的接收文件行。`local_path` 是收件箱条目的落盘位置事实源（OPFS 句柄路径推导不出来），
    /// `local_dir` 一致才让 `content_root_of` 解析出真实容器目录。
    fn received_file(
        session_id: Uuid,
        file_id: i32,
        name: &str,
        relative_path: &str,
        checksum: &str,
        size: i64,
    ) -> entity::transfer_file::Model {
        entity::transfer_file::Model {
            id: file_id,
            session_id,
            file_id,
            name: name.to_string(),
            relative_path: relative_path.to_string(),
            size,
            checksum: checksum.to_string(),
            status: entity::FileStatus::Completed,
            transferred_bytes: size,
            total_chunks: 1,
            completed_chunks: vec![1],
            completed_ranges: "[]".to_string(),
            source_path: None,
            local_path: Some(format!("{SAVE_ROOT}/{relative_path}")),
            local_dir: Some(SAVE_ROOT.to_string()),
            outboard: None,
        }
    }

    /// 单文件条目的便捷构造：给定来源名与 `received_at`，落一条条目并返回其详情。
    async fn seed_item(
        table: &WebInboxTable,
        peer_name: &str,
        received_at: i64,
        name: &str,
        relative_path: &str,
    ) -> InboxItemDetail {
        let session_id = Uuid::new_v4();
        let session = receive_session(session_id, peer_name, received_at);
        let files = vec![received_file(
            session_id,
            0,
            name,
            relative_path,
            "checksum-0",
            5,
        )];
        table
            .ensure_from_session(&session, &files)
            .await
            .expect("建收件箱条目")
            .expect("已完成接收会话必须产出条目")
    }

    fn ids(items: &[InboxItemDetail]) -> Vec<Uuid> {
        items.iter().map(|detail| detail.item.id).collect()
    }

    fn hit_ids(hits: &[InboxSearchHit]) -> Vec<Uuid> {
        hits.iter().map(|hit| hit.id).collect()
    }

    #[wasm_bindgen_test]
    async fn text_delivery_is_idempotent_conflict_safe_and_deleted_with_its_inbox_record() {
        let table = WebInboxTable::default();
        let delivery_id = Uuid::new_v4();
        let incoming = |body: &str| TextDeliveryRecord {
            delivery_id,
            direction: entity::TextDeliveryDirection::Receive,
            peer_id: "peer-a".into(),
            peer_name: "Alice".into(),
            body: body.into(),
            status: entity::TextDeliveryStatus::Delivered,
            failure: None,
            attempt_count: 1,
            created_at: 1,
            updated_at: 1,
        };

        let first = table
            .persist_text_delivery(incoming("first body"))
            .await
            .expect("首次写入文本收件箱");
        let replay = table
            .persist_text_delivery(incoming("first body"))
            .await
            .expect("同一投递重放必须收敛");
        assert_eq!(
            first.item.id, replay.item.id,
            "重放不得创建第二个 Inbox 条目"
        );
        assert_eq!(table.list(true).len(), 1);
        assert!(
            table
                .persist_text_delivery(incoming("tampered"))
                .await
                .is_err(),
            "同一 id 不得承载不同正文"
        );

        table
            .delete_record(first.item.id)
            .await
            .expect("删除文本收件箱记录");
        assert!(
            table.text_delivery(delivery_id).is_none(),
            "删除必须移除正文"
        );
        assert!(table.detail(first.item.id).is_none());
    }

    /// 幂等：同一个会话反复 `ensure` 只能有一条条目。
    ///
    /// 它红了意味着重连后重放一次完成事件（或用户重开一次详情页）就会在收件箱里
    /// 多出一份同样的东西，且两份的 `id` 不同——「打开」「归档」「删除」从此各作用于一份。
    #[wasm_bindgen_test]
    async fn ensure_from_session_is_idempotent() {
        let table = WebInboxTable::default();
        let session_id = Uuid::new_v4();
        let session = receive_session(session_id, "小明的 Mac", 1_000);
        let files = vec![
            received_file(session_id, 0, "hello.txt", "hello.txt", "checksum-0", 12),
            received_file(
                session_id,
                1,
                "readme.md",
                "docs/readme.md",
                "checksum-1",
                8,
            ),
        ];

        let first = table
            .ensure_from_session(&session, &files)
            .await
            .expect("首次建条目")
            .expect("已完成接收应产出条目");
        let second = table
            .ensure_from_session(&session, &files)
            .await
            .expect("再次 ensure")
            .expect("应返回同一条");

        assert_eq!(first.item.id, second.item.id, "同一会话只能有一个条目 id");
        assert_eq!(table.list(true).len(), 1, "表里只应有一条条目");
        assert_eq!(first.item.transfer_session_id, Some(session_id));
        assert_eq!(first.item.item_count, 2);
        assert_eq!(first.files.len(), 2);
        assert_eq!(first.item.received_at, 1_000, "received_at 取会话完成时刻");
        assert_eq!(first.item.root_path.as_deref(), Some(SAVE_ROOT));
        assert!(first.files.iter().all(|file| !file.missing));
        assert!(
            first.transfer.is_none(),
            "收件箱表看不见会话表，投影这一格必须留给 WebTransferStore 补"
        );
    }

    /// 内容指纹与标题两端同源：这里用的输入与 `swarmdrop_transfer::inbox` 里
    /// `inbox_content_hash_known_vector` 的**同一组**，期望值也是同一个十六进制常量。
    ///
    /// 它红了意味着浏览器与桌面对同一批文件算出了不同的 `content_hash`——跨端去重的唯一
    /// 判据当场作废，而这种漂移没有任何运行时报错，只会表现为「同一份东西在两端被认成两份」。
    #[wasm_bindgen_test]
    async fn content_hash_matches_shared_known_vector() {
        let table = WebInboxTable::default();
        let session_id = Uuid::new_v4();
        let session = receive_session(session_id, "小明的 Mac", 1_000);
        let files = vec![
            received_file(session_id, 0, "hello.txt", "hello.txt", "checksum-0", 12),
            received_file(
                session_id,
                1,
                "readme.md",
                "docs/readme.md",
                "checksum-1",
                8,
            ),
        ];

        let detail = table
            .ensure_from_session(&session, &files)
            .await
            .expect("建条目")
            .expect("条目");

        assert_eq!(
            detail.item.content_hash.as_deref(),
            Some("d574ff2b0c617b92d1d827e0f3cf5410d2d3e5c1a165393969338c15f67c9a04"),
            "Web 侧构造条目必须落到与 SQL 侧同一个已知向量上"
        );
        // 标题列存的是**首个文件名**，不是渲染好的句子。「等 N 个文件」由三端各自的
        // catalog 生成——落库的值必须与界面语言无关，否则历史条目会永久冻结在写入时的
        // locale。取哪一个文件名仍是共享规则（`inbox_primary_file_name`），不能在
        // Web 侧另写一份。
        assert_eq!(detail.item.title, "hello.txt");
        assert_eq!(detail.item.item_count, 2, "「等 N 个」的 N 由这一列提供");
    }

    /// **指纹只取决于 `file_id`，与对端 manifest 的排列无关。**
    ///
    /// `file_id` 是对端声明的字段（从 0 递增只是诚实发送端的自律），乱序的 offer 构造得出来。
    /// 上一条测试喂的是已排好序的 manifest，所以它**测不出**「这一端有没有排序」——
    /// 两端各按自己的本地顺序（SQL 的 rowid / 这里的数组下标）喂给共享规则时，同一批文件
    /// 会算出不同的指纹，而 `content_hash` 是跨端去重的唯一判据。
    ///
    /// 这里把同一组文件倒序喂进来，期望值仍是那个已知向量。
    /// SQL 侧的对应约束是 `content_hash_is_independent_of_manifest_order`。
    #[wasm_bindgen_test]
    async fn content_hash_is_independent_of_manifest_order() {
        let table = WebInboxTable::default();
        let session_id = Uuid::new_v4();
        let session = receive_session(session_id, "小明的 Mac", 1_000);
        // 与 `content_hash_matches_shared_known_vector` 同一组文件，**manifest 顺序相反**。
        let files = vec![
            received_file(
                session_id,
                1,
                "readme.md",
                "docs/readme.md",
                "checksum-1",
                8,
            ),
            received_file(session_id, 0, "hello.txt", "hello.txt", "checksum-0", 12),
        ];

        let detail = table
            .ensure_from_session(&session, &files)
            .await
            .expect("建条目")
            .expect("条目");

        assert_eq!(
            detail.item.content_hash.as_deref(),
            Some("d574ff2b0c617b92d1d827e0f3cf5410d2d3e5c1a165393969338c15f67c9a04"),
            "manifest 倒序不得改变指纹——否则同一份东西在两端会被认成两份"
        );
        assert_eq!(
            detail.item.title, "hello.txt",
            "首文件同样按 file_id 取，不是 manifest 的第一个"
        );
    }

    /// 跨「重启」存活：写穿 IndexedDB → 新实例 `load()` → 条目逐字段一致。
    ///
    /// 断言比的是**整条详情的 JSON**，而不是挑几个字段——`PersistedInboxItem` 的 remote derive
    /// 漏掉任何一列（或 entity 新增列时没同步）都会在这里露出来。写成挑字段的断言就测不出
    /// 「新加的列没落库」这一类静默丢数据。
    ///
    /// 三个可选字段刻意先改成非默认值（`last_opened_at` / `archived_at` / 文件级 `missing`），
    /// 否则全 `None` / `false` 的往返对「字段丢了」是不敏感的。
    #[wasm_bindgen_test]
    async fn item_survives_reload_from_indexed_db() {
        let table = WebInboxTable::default();
        let session_id = Uuid::new_v4();
        let session = receive_session(session_id, "Alice 的工作站", 1_700_000_000_000);
        let files = vec![
            received_file(session_id, 0, "hello.txt", "hello.txt", "checksum-0", 12),
            received_file(
                session_id,
                1,
                "readme.md",
                "docs/readme.md",
                "checksum-1",
                8,
            ),
        ];
        let item_id = table
            .ensure_from_session(&session, &files)
            .await
            .expect("建条目")
            .expect("条目")
            .item
            .id;
        table.mark_opened(item_id).await.expect("标记已打开");
        table.archive(item_id, true).await.expect("归档");
        table
            .mark_file_missing(item_id, 1, true)
            .await
            .expect("标记文件缺失");

        let before = table.detail(item_id).expect("刷新前详情");
        let reloaded = WebInboxTable::load().await;
        let after = reloaded.detail(item_id).expect("刷新后条目必须还在");

        assert_eq!(
            serde_json::to_value(&before).expect("序列化刷新前详情"),
            serde_json::to_value(&after).expect("序列化刷新后详情"),
            "IndexedDB 往返后条目必须逐字段一致"
        );
        // 上面的等值断言只保证「往返没变」，这几条保证往返的**内容**本身不是空壳。
        assert!(after.item.last_opened_at.is_some());
        assert!(after.item.archived_at.is_some());
        assert_eq!(after.files.len(), 2);
        assert!(after.files[1].missing);
        assert_eq!(
            after.files[1].local_path,
            "/inbox-table-test/docs/readme.md"
        );

        // 同一个 IndexedDB 库被所有 wasm 测试共用，`load()` 会读到全部记录——
        // 本测试按 id 断言故不受污染，但仍清掉自己的 key，别给别人留垃圾。
        SendWrapper::new(idb::delete(idb::INBOX_STORE, &item_id.to_string()))
            .await
            .expect("清理测试记录");
    }

    /// 列表的可见性与顺序：软删项一律不可见、归档项按开关、`received_at` 倒序。
    ///
    /// 它红了意味着收件箱首屏要么把用户删掉的东西又摆出来，要么顺序随 `HashMap` 迭代序乱跳
    /// （「最近收到的」变成随机 N 条）。
    ///
    /// 与下面的 search 测试**不是重复**：`list` 与 `search` 各自实现了一遍
    /// `filter(visible)` + `sort_by_received_at_desc`，删掉其中一处，另一处的测试照绿。
    #[wasm_bindgen_test]
    async fn list_orders_by_received_at_desc_and_applies_visibility() {
        let table = WebInboxTable::default();
        let old = seed_item(&table, "Carol", 100, "a.pdf", "a.pdf")
            .await
            .item
            .id;
        let mid = seed_item(&table, "Carol", 200, "b.pdf", "b.pdf")
            .await
            .item
            .id;
        let new = seed_item(&table, "Carol", 300, "c.pdf", "c.pdf")
            .await
            .item
            .id;

        assert_eq!(
            ids(&table.list(false)),
            vec![new, mid, old],
            "应按 received_at 倒序"
        );

        table.archive(mid, true).await.expect("归档");
        assert_eq!(
            ids(&table.list(false)),
            vec![new, old],
            "默认不列已归档条目"
        );
        assert_eq!(
            ids(&table.list(true)),
            vec![new, mid, old],
            "include_archived 时已归档条目回到原位"
        );

        table.delete_record(old).await.expect("软删");
        assert_eq!(ids(&table.list(false)), vec![new]);
        assert_eq!(
            ids(&table.list(true)),
            vec![new, mid],
            "软删项在 include_archived 下同样不可见"
        );

        table.archive(mid, false).await.expect("取消归档");
        assert_eq!(
            ids(&table.list(false)),
            vec![new, mid],
            "取消归档必须真的清掉 archived_at"
        );
    }

    /// 检索：命中判据覆盖 files_text、结果按 `received_at` 倒序、`limit` 截断、
    /// 软删不返回、归档按开关。
    ///
    /// 查询词只出现在文件的 `relative_path` 里（标题是文件名、来源名是设备名，两者都不含它），
    /// 于是这条测试同时钉住 `item_facts` 把**收件箱文件行**摊成了正确的中立视图——摊错了的话
    /// 「按目录名搜」会静默搜不到，而按标题搜照样能中，人工验收根本发现不了。
    #[wasm_bindgen_test]
    async fn search_matches_files_text_and_applies_limit_and_visibility() {
        let table = WebInboxTable::default();
        let old = seed_item(&table, "Dave", 100, "a.pdf", "invoices/a.pdf")
            .await
            .item
            .id;
        let mid = seed_item(&table, "Dave", 200, "b.pdf", "invoices/b.pdf")
            .await
            .item
            .id;
        let new = seed_item(&table, "Dave", 300, "c.pdf", "invoices/c.pdf")
            .await
            .item
            .id;

        let hits = table.search("invoices", 10, false);
        assert_eq!(
            hit_ids(&hits),
            vec![new, mid, old],
            "只出现在 relative_path 里的词也必须命中，且按 received_at 倒序"
        );
        assert_eq!(hits[0].files.len(), 1);
        assert_eq!(hits[0].files[0].relative_path, "invoices/c.pdf");
        // 「invoices」只在 relative_path 里，不在标题（单文件条目的标题就是 `c.pdf`）也不在
        // 来源名里——所以这是**该给**片段的那种命中：用户要找的东西不在条目行上直接可见。
        // 反过来命中标题/来源名时 `inbox_snippet` 返回 None，见它的文档与
        // `swarmdrop-transfer` 的 `snippet_only_for_file_hits`。
        assert!(
            hits[0]
                .snippet
                .as_deref()
                .is_some_and(|s| s.contains("invoices")),
            "只命中文件文本时要带可读片段"
        );

        assert_eq!(
            hit_ids(&table.search("invoices", 2, false)),
            vec![new, mid],
            "limit 应在倒序之后截断（截的是最旧的那条）"
        );

        table.delete_record(old).await.expect("软删");
        table.archive(mid, true).await.expect("归档");
        assert_eq!(hit_ids(&table.search("invoices", 10, false)), vec![new]);
        assert_eq!(
            hit_ids(&table.search("invoices", 10, true)),
            vec![new, mid],
            "include_archived 只放行归档项，软删项始终不返回"
        );

        assert!(table.search("", 10, true).is_empty(), "空查询恒不命中");
        assert!(table.search("   ", 10, true).is_empty(), "全空白同上");
        assert!(table.search("zzz不存在", 10, true).is_empty());
    }

    /// 归属校验：`file_id` 在 Web 侧只是**条目内序号**，每个条目都从 0 开始编。
    ///
    /// 它红了意味着「标记文件缺失」会静默作用到另一条条目的同序号文件上，或者对一个根本
    /// 不存在的序号返回 `Ok`——两种都让 UI 的 missing 标记与磁盘现实脱节。
    #[wasm_bindgen_test]
    async fn mark_file_missing_requires_file_to_belong_to_item() {
        let table = WebInboxTable::default();
        let single = seed_item(&table, "Zoe", 100, "我的.pdf", "我的.pdf")
            .await
            .item
            .id;

        // 另一条条目有两个文件，于是序号 1 在整张表里是存在的——但不属于 `single`。
        let other_session = Uuid::new_v4();
        let other = table
            .ensure_from_session(
                &receive_session(other_session, "Zoe", 200),
                &[
                    received_file(other_session, 0, "别人的0.pdf", "别人的0.pdf", "c0", 5),
                    received_file(other_session, 1, "别人的1.pdf", "别人的1.pdf", "c1", 5),
                ],
            )
            .await
            .expect("建条目")
            .expect("条目")
            .item
            .id;

        assert!(
            table.mark_file_missing(single, 1, true).await.is_err(),
            "不属于该条目的 file_id 必须报错，不能改到别人的文件上"
        );
        assert!(
            table
                .detail(other)
                .expect("另一条条目还在")
                .files
                .iter()
                .all(|file| !file.missing),
            "另一条条目的文件不该被牵连"
        );
        assert!(
            table
                .mark_file_missing(Uuid::new_v4(), 0, true)
                .await
                .is_err(),
            "条目不存在必须报错"
        );

        table
            .mark_file_missing(single, 0, true)
            .await
            .expect("自己的文件应可标记");
        let detail = table.detail(single).expect("条目");
        assert!(detail.files[0].missing);
        assert!(detail.item.missing, "条目级 missing 由文件行聚合");
    }

    /// 只有「已完成的接收」才进收件箱，其余一律 `Ok(None)`——**不是错误**。
    ///
    /// 它红了意味着发出去的文件、取消掉的接收、还在传的接收都会作为「已收到的内容」
    /// 出现在收件箱里，且都指向一个不存在或不完整的落盘位置。
    #[wasm_bindgen_test]
    async fn ensure_from_session_skips_non_completed_receives() {
        let table = WebInboxTable::default();

        let send_id = Uuid::new_v4();
        let mut send = receive_session(send_id, "X", 100);
        send.direction = entity::TransferDirection::Send;

        let running_id = Uuid::new_v4();
        let mut running = receive_session(running_id, "X", 100);
        running.phase = entity::TransferPhase::Active;
        running.terminal_reason = None;

        let cancelled_id = Uuid::new_v4();
        let mut cancelled = receive_session(cancelled_id, "X", 100);
        cancelled.terminal_reason = Some(entity::TerminalReason::Cancelled);

        for (session, session_id) in [
            (send, send_id),
            (running, running_id),
            (cancelled, cancelled_id),
        ] {
            let files = vec![received_file(session_id, 0, "a.bin", "a.bin", "sum", 5)];
            assert!(
                table
                    .ensure_from_session(&session, &files)
                    .await
                    .expect("非完成态不该报错")
                    .is_none(),
                "只有 Receive + Terminal + Completed 才产出条目"
            );
        }
        assert!(table.list(true).is_empty());
    }

    /// 已完成接收缺 `save_path` 或缺 `local_path` 是数据异常，必须**显式报错**，
    /// 且不能留下半条条目。
    ///
    /// 它红了意味着收件箱里会出现一条打不开的条目（root_path 为空 / 文件无落盘位置），
    /// 而错误被吞掉后没有任何地方能查到它是怎么来的。
    #[wasm_bindgen_test]
    async fn ensure_from_session_rejects_incomplete_receive_rows() {
        let table = WebInboxTable::default();

        let no_save_path_id = Uuid::new_v4();
        let mut no_save_path = receive_session(no_save_path_id, "X", 100);
        no_save_path.save_path = None;
        assert!(
            table
                .ensure_from_session(
                    &no_save_path,
                    &[received_file(
                        no_save_path_id,
                        0,
                        "a.bin",
                        "a.bin",
                        "sum",
                        5
                    )],
                )
                .await
                .is_err(),
            "已完成接收缺保存位置必须报错"
        );

        let no_local_path_id = Uuid::new_v4();
        let mut orphan_file = received_file(no_local_path_id, 0, "a.bin", "a.bin", "sum", 5);
        orphan_file.local_path = None;
        assert!(
            table
                .ensure_from_session(&receive_session(no_local_path_id, "X", 100), &[orphan_file],)
                .await
                .is_err(),
            "已完成接收的文件缺落盘路径必须报错"
        );

        assert!(
            table.list(true).is_empty(),
            "报错的会话不该留下半条条目（插入必须在全部文件行都构造成功之后）"
        );
    }
}
