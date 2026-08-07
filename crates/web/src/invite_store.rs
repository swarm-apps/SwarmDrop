//! 邀请注册表的 IndexedDB 落盘实现（`InviteStore` 端口的 Web 后端）。
//!
//! 与 [`crate::store`]（传输会话）同一套路子：**内存不留副本，直接读写 IndexedDB** ——
//! 邀请注册表的内存态由 `InviteRegistry` 自己持有，这里只做持久化，不必再缓存一层。
//!
//! 两条与桌面不同的约束：
//!
//! - IndexedDB 的 `JsFuture` 是 `!Send`，用 `SendWrapper` 裹住以满足 `#[async_trait]`
//!   的 Send 约束（单线程 wasm 下跨线程 panic 永不触发，见
//!   `dev-notes/knowledge/storage-abstraction.md`）。
//! - 端口方法**不返回错误**，所以每个失败就地降级成 `tracing::warn` 并继续：内存表是
//!   一次性消费的权威判定点，写库失败不该让配对失败（见 invite-persistence design D2）。
//!
//! 落库形态只有 capability 的 sha256 与元数据，**没有 capability 明文、没有邀请全串**
//! （design D4）——刷新后邀请列表能显示状态，但拼不回原始链接。

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use swarmdrop_invite::{
    InviteRecord, InviteStore, PersistedInviteState, capability_hash_from_hex,
    capability_hash_to_hex,
};
use swarmdrop_net_base::NodeId;

use crate::idb;

/// IndexedDB 里的一行。字段名即 wire 格式契约 —— 改名要考虑存量库。
#[derive(Serialize, Deserialize)]
struct StoredInvite {
    /// `sha256(capability)` 的小写 hex，同时是 object store 的 key。
    capability_hash: String,
    /// 发起方 NodeId（base58）。
    inviter_id: String,
    expires_at: u64,
    /// `"pending"` / `"consumed"`。
    state: String,
    created_at: u64,
}

/// `InviteStore` 的 IndexedDB 实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct IdbInviteStore;

#[async_trait]
impl InviteStore for IdbInviteStore {
    async fn load_all(&self) -> Vec<InviteRecord> {
        let raw = match SendWrapper::new(idb::get_all(idb::INVITE_STORE)).await {
            Ok(values) => values,
            Err(e) => {
                tracing::warn!("读取邀请注册表失败，按无记录继续: {e:?}");
                return Vec::new();
            }
        };
        // **必须与 `upsert` 的写法对称**：那边是 `serde_json::to_string` + `put_string`，
        // 存进去的是一个 JSON **字符串**，不是结构化对象。此前这里用
        // `serde_wasm_bindgen::from_value` 直接当对象读，于是每一行都以
        // `invalid type: string "...", expected struct StoredInvite` 被丢掉——
        // 结果是**已发出的邀请跨刷新全部消失**：用户看不到自己发过什么，也就无从撤销，
        // 而那正是「邀请可撤销」这个能力唯一的入口。
        //
        // 症状极隐蔽：写入是成功的（IndexedDB 里躺着完整记录），只有读回来时静默丢弃，
        // 且丢弃走的是「单行坏了只丢这一行」这条本来正确的容错路径。
        // 收件箱表（`inbox.rs`）一直是对称的，可作对照。
        raw.into_iter()
            .filter_map(|value| {
                let json = value.as_string()?;
                match serde_json::from_str(&json) {
                    Ok(stored) => to_record(stored),
                    Err(e) => {
                        // 单行坏了只丢这一行：邀请是短时凭证，最坏结果是用户重新生成一次
                        tracing::warn!("邀请记录反序列化失败，丢弃该行: {e}");
                        None
                    }
                }
            })
            .collect()
    }

    async fn upsert(&self, record: InviteRecord) -> bool {
        let key = capability_hash_to_hex(&record.capability_hash);
        let stored = StoredInvite {
            capability_hash: key.clone(),
            inviter_id: record.inviter_id.to_string(),
            expires_at: record.expires_at,
            state: match record.state {
                PersistedInviteState::Pending => "pending".to_owned(),
                PersistedInviteState::Consumed => "consumed".to_owned(),
                PersistedInviteState::Revoked => "revoked".to_owned(),
            },
            created_at: record.created_at,
        };
        let json = match serde_json::to_string(&stored) {
            Ok(json) => json,
            Err(e) => {
                tracing::warn!("邀请记录序列化失败: {e}");
                return false;
            }
        };
        match SendWrapper::new(idb::put_string(idb::INVITE_STORE, &key, &json)).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("写入邀请记录失败（调用方将据此 fail-closed）: {e:?}");
                false
            }
        }
    }

    async fn remove(&self, capability_hash: [u8; 32]) -> bool {
        let key = capability_hash_to_hex(&capability_hash);
        match SendWrapper::new(idb::delete(idb::INVITE_STORE, &key)).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("删除邀请记录失败: {e:?}");
                false
            }
        }
    }

    async fn prune_expired(&self, now: u64) {
        // IndexedDB 没有「按条件批量删」，只能读回来逐条删。邀请表规模是「本机未过期的
        // 已发出邀请」——个位数量级，全读一遍可以忽略。
        for record in self.load_all().await {
            if now >= record.expires_at {
                let _ = self.remove(record.capability_hash).await;
            }
        }
    }
}

fn to_record(stored: StoredInvite) -> Option<InviteRecord> {
    let capability_hash = capability_hash_from_hex(&stored.capability_hash).or_else(|| {
        tracing::warn!("邀请记录的 capability_hash 无法解析，丢弃该行");
        None
    })?;
    let inviter_id = stored
        .inviter_id
        .parse::<NodeId>()
        .inspect_err(|e| {
            tracing::warn!("邀请记录的 inviter_id 无法解析，丢弃该行: {e}");
        })
        .ok()?;
    let state = match stored.state.as_str() {
        "pending" => PersistedInviteState::Pending,
        "consumed" => PersistedInviteState::Consumed,
        "revoked" => PersistedInviteState::Revoked,
        other => {
            tracing::warn!("邀请记录状态未知，丢弃该行: {other}");
            return None;
        }
    };
    Some(InviteRecord {
        capability_hash,
        inviter_id,
        expires_at: stored.expires_at,
        state,
        created_at: stored.created_at,
    })
}
