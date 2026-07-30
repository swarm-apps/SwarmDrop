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
use swarmdrop_invite::{InviteRecord, InviteStore, PersistedInviteState};
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
        raw.into_iter()
            .filter_map(|value| match serde_wasm_bindgen::from_value(value) {
                Ok(stored) => to_record(stored),
                Err(e) => {
                    // 单行坏了只丢这一行：邀请是短时凭证，最坏结果是用户重新生成一次
                    tracing::warn!("邀请记录反序列化失败，丢弃该行: {e}");
                    None
                }
            })
            .collect()
    }

    async fn upsert(&self, record: InviteRecord) -> bool {
        let key = hex_lower(&record.capability_hash);
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
        let key = hex_lower(&capability_hash);
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
    let capability_hash = from_hex_lower(&stored.capability_hash)?;
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

fn hex_lower(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn from_hex_lower(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        tracing::warn!("邀请记录的 capability_hash 长度异常: {}", text.len());
        return None;
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}
