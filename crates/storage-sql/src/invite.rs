//! 邀请注册表的 SQL 落盘实现（`InviteStore` 端口的桌面 / 移动后端）。
//!
//! **写方法返回「成没成」，调用方据此 fail-closed。** 早期这里写的是「端口方法都不返回
//! 错误，写库失败时正确的用户可见行为就是配对照样成功」—— 那个判断已被推翻（详见
//! `swarmdrop_invite::store` 的模块文档与 `dev-notes/knowledge/storage-abstraction.md`）：
//! `register` 已经把 `Pending` 写进库了，此后任何一次写穿失败都留下同一个结果 ——
//! 库里仍是 `Pending`，重启 `load` 把它放回内存，那份一次性凭证于是能被消费第二次。
//! 所以 `upsert` / `remove` 报 `false`，`try_consume` 在收到 `false` 时中止本次配对。
//!
//! 读路径仍就地降级（`load_all` 失败按「无记录」继续）：邀请是短时凭证，最坏结果是用户
//! 重新生成一条，比启动失败好。
//!
//! 表结构见 `entity::pair_invite`，建表在 `migration::m20260805_000001_init`。
//! **表里没有 capability 明文，也没有邀请全串**，只有前者的 sha256。

use async_trait::async_trait;
use entity::pair_invite::{Column, InviteState as DbInviteState};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, IntoActiveModel, QueryFilter,
    sea_query::OnConflict,
};
use swarmdrop_invite::{InviteRecord, InviteStore, PersistedInviteState};
use swarmdrop_net_base::NodeId;

/// `InviteStore` 的 SeaORM 实现。
pub struct SqlInviteStore {
    db: DatabaseConnection,
}

impl SqlInviteStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl InviteStore for SqlInviteStore {
    async fn load_all(&self) -> Vec<InviteRecord> {
        match entity::PairInvite::find().all(&self.db).await {
            Ok(rows) => rows.iter().filter_map(to_record).collect(),
            Err(e) => {
                tracing::warn!("读取邀请注册表失败，按无记录继续: {e}");
                Vec::new()
            }
        }
    }

    async fn upsert(&self, record: InviteRecord) -> bool {
        let model = entity::pair_invite::ActiveModel::builder()
            .set_capability_hash(hex_lower(&record.capability_hash))
            .set_inviter_id(entity::PeerId(record.inviter_id.to_string()))
            .set_expires_at(to_i64(record.expires_at))
            .set_state(to_db_state(record.state))
            .set_created_at(to_i64(record.created_at))
            .into_active_model();

        // 状态变更（Pending → Consumed）走同一入口，故按主键冲突时更新
        let result = entity::PairInvite::insert(model)
            .on_conflict(
                OnConflict::column(Column::CapabilityHash)
                    .update_columns([Column::State, Column::ExpiresAt])
                    .to_owned(),
            )
            .exec(&self.db)
            .await;
        match result {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("写入邀请记录失败（调用方将据此 fail-closed）: {e}");
                false
            }
        }
    }

    async fn remove(&self, capability_hash: [u8; 32]) -> bool {
        let result = entity::PairInvite::delete_by_id(hex_lower(&capability_hash))
            .exec(&self.db)
            .await;
        match result {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("删除邀请记录失败: {e}");
                false
            }
        }
    }

    async fn prune_expired(&self, now: u64) {
        let result = entity::PairInvite::delete_many()
            .filter(Column::ExpiresAt.lte(to_i64(now)))
            .exec(&self.db)
            .await;
        if let Err(e) = result {
            tracing::warn!("清理过期邀请失败: {e}");
        }
    }
}

/// 行 → 端口记录。**无法解析的行直接丢弃**（记一条 warn）：邀请是短时凭证，
/// 与其让一条脏数据把整批读取拖垮，不如少认一条，最坏结果是用户重新生成一次。
fn to_record(row: &entity::pair_invite::Model) -> Option<InviteRecord> {
    let capability_hash = from_hex_lower(&row.capability_hash)?;
    let inviter_id = match row.inviter_id.as_str().parse::<NodeId>() {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(
                "邀请记录的 inviter_id 无法解析，丢弃该行: {} ({e})",
                row.inviter_id
            );
            return None;
        }
    };
    Some(InviteRecord {
        capability_hash,
        inviter_id,
        expires_at: row.expires_at.max(0) as u64,
        state: from_db_state(&row.state),
        created_at: row.created_at.max(0) as u64,
    })
}

fn to_db_state(state: PersistedInviteState) -> DbInviteState {
    match state {
        PersistedInviteState::Pending => DbInviteState::Pending,
        PersistedInviteState::Consumed => DbInviteState::Consumed,
        PersistedInviteState::Revoked => DbInviteState::Revoked,
    }
}

fn from_db_state(state: &DbInviteState) -> PersistedInviteState {
    match state {
        DbInviteState::Pending => PersistedInviteState::Pending,
        DbInviteState::Consumed => PersistedInviteState::Consumed,
        DbInviteState::Revoked => PersistedInviteState::Revoked,
    }
}

/// Unix 秒的 u64 → i64。`expires_at` 是 `now + 86400` 量级，溢出不可能发生，
/// 真溢出了钳到 `i64::MAX`（表现为「永不过期」）也比 panic 好。
fn to_i64(secs: u64) -> i64 {
    i64::try_from(secs).unwrap_or(i64::MAX)
}

fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn from_hex_lower(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        tracing::warn!(
            "邀请记录的 capability_hash 长度异常，丢弃该行: {}",
            text.len()
        );
        return None;
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        // `get` 而非 `&text[a..b]`：长度校验的是**字节**数，含多字节字符的 64 字节串
        // 会让切片落在字符中间而 panic。这一列由本实现自己写入（纯 hex），但读回来的
        // 是外部可改的库文件，不值得赌。
        *slot = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectOptions, Database};
    use swarmdrop_net_base::SecretKey;

    async fn make_db() -> DatabaseConnection {
        let mut opt = ConnectOptions::new("sqlite::memory:");
        opt.max_connections(1)
            .min_connections(1)
            .sqlx_logging(false);
        let db = Database::connect(opt).await.expect("connect sqlite memory");
        Migrator::up(&db, None).await.expect("run migrations");
        db
    }

    fn record(hash_byte: u8, expires_at: u64, state: PersistedInviteState) -> InviteRecord {
        InviteRecord {
            capability_hash: [hash_byte; 32],
            inviter_id: SecretKey::generate().node_id(),
            expires_at,
            state,
            created_at: 1_700_000_000,
        }
    }

    #[tokio::test]
    async fn roundtrip_preserves_fields() {
        let store = SqlInviteStore::new(make_db().await);
        let original = record(0xab, 1_700_086_400, PersistedInviteState::Pending);
        store.upsert(original.clone()).await;

        let loaded = store.load_all().await;
        assert_eq!(loaded, vec![original]);
    }

    #[tokio::test]
    async fn upsert_updates_state_in_place() {
        let store = SqlInviteStore::new(make_db().await);
        let pending = record(0x01, 1_700_086_400, PersistedInviteState::Pending);
        store.upsert(pending.clone()).await;

        let consumed = InviteRecord {
            state: PersistedInviteState::Consumed,
            ..pending.clone()
        };
        store.upsert(consumed.clone()).await;

        // 同一 capability 只留一行，且状态已变
        let loaded = store.load_all().await;
        assert_eq!(loaded, vec![consumed]);
    }

    #[tokio::test]
    async fn remove_deletes_single_row() {
        let store = SqlInviteStore::new(make_db().await);
        store
            .upsert(record(0x01, 1_700_086_400, PersistedInviteState::Pending))
            .await;
        store
            .upsert(record(0x02, 1_700_086_400, PersistedInviteState::Pending))
            .await;

        store.remove([0x01; 32]).await;

        let loaded = store.load_all().await;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].capability_hash, [0x02; 32]);
    }

    /// 清理只按过期时间，不看状态：**已消费但未过期**的记录要留着，让邀请列表能显示
    /// 「已被使用」。（安全不依赖这条 —— 注册表查不到就是拒绝。）
    #[tokio::test]
    async fn prune_keeps_consumed_until_expiry() {
        let store = SqlInviteStore::new(make_db().await);
        let consumed_alive = record(0x01, 2_000_000_000, PersistedInviteState::Consumed);
        let pending_expired = record(0x02, 1_600_000_000, PersistedInviteState::Pending);
        let consumed_expired = record(0x03, 1_600_000_000, PersistedInviteState::Consumed);
        store.upsert(consumed_alive.clone()).await;
        store.upsert(pending_expired).await;
        store.upsert(consumed_expired).await;

        store.prune_expired(1_700_000_000).await;

        let loaded = store.load_all().await;
        assert_eq!(
            loaded,
            vec![consumed_alive],
            "只有过期的该被清掉，已消费但未过期的必须留着"
        );
    }

    #[test]
    fn hex_roundtrip() {
        let bytes = [0x00, 0x0f, 0xff, 0xa5]
            .iter()
            .cycle()
            .take(32)
            .copied()
            .collect::<Vec<_>>();
        let mut fixed = [0u8; 32];
        fixed.copy_from_slice(&bytes);
        assert_eq!(from_hex_lower(&hex_lower(&fixed)), Some(fixed));
    }

    #[test]
    fn malformed_hex_is_rejected() {
        assert_eq!(from_hex_lower("short"), None);
        assert_eq!(from_hex_lower(&"z".repeat(64)), None);
    }
}
