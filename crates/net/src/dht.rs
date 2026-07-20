//! [`Dht`] 子 API：Kademlia record 存取与 provider 宣告。
//!
//! 独立于 AddressLookup——分享码/在线宣告的 record 是**应用数据**不是地址
//! 解析；反过来「在线宣告 lookup」由上层基于本 API 实现 `AddressLookup`
//! trait（扩展点的验证用例）。

use std::time::Duration;

use sha2::{Digest, Sha256};
use swarmdrop_net_base::NodeId;
use tokio::sync::{mpsc, oneshot};

use crate::actor::ActorMessage;

/// DHT 键（SHA256 派生，迁自旧栈 `dht_key.rs`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DhtKey([u8; 32]);

impl DhtKey {
    /// 命名空间键：`SHA256(len(namespace) ++ namespace ++ id)`。
    ///
    /// 长度前缀做域分离——纯拼接下 `("ab","c")` 与 `("a","bc")` 会散列到
    /// 同一 key（旧栈 dht_key.rs 的理论缺陷，wire v2 顺手修正）。
    ///
    /// 例：分享码 `DhtKey::namespaced("/swarmdrop/share-code/", code.as_bytes())`。
    pub fn namespaced(namespace: &str, id: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update((namespace.len() as u64).to_be_bytes());
        hasher.update(namespace.as_bytes());
        hasher.update(id);
        Self(hasher.finalize().into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// 从 DHT 取回的 record。
#[derive(Debug, Clone)]
pub struct DhtRecord {
    pub value: Vec<u8>,
    /// 发布者身份（配对流程用它当对端 peer id）。
    pub publisher: Option<NodeId>,
}

/// DHT 操作失败。
#[derive(Debug, thiserror::Error)]
pub enum DhtError {
    #[error("endpoint is closed")]
    Closed,
    #[error("record not found")]
    NotFound,
    #[error("dht query failed: {0}")]
    QueryFailed(String),
}

/// DHT 命令（actor 转成 kad 查询，QueryId 挂 pending 表）。
pub(crate) enum DhtCommand {
    Bootstrap {
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    Put {
        key: DhtKey,
        value: Vec<u8>,
        ttl: Option<Duration>,
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    Get {
        key: DhtKey,
        reply: oneshot::Sender<Result<DhtRecord, DhtError>>,
    },
    /// 本地移除（停止 re-publish；网络上等 TTL 自然过期）。
    Remove {
        key: DhtKey,
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    Provide {
        key: DhtKey,
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    StopProvide {
        key: DhtKey,
        reply: oneshot::Sender<Result<(), DhtError>>,
    },
    Providers {
        key: DhtKey,
        reply: oneshot::Sender<Result<Vec<NodeId>, DhtError>>,
    },
}

/// DHT 操作句柄（`Endpoint::dht()` 返回；仅在 Builder 启用 DHT 时存在）。
#[derive(Debug, Clone)]
pub struct Dht {
    actor_tx: mpsc::Sender<ActorMessage>,
}

impl Dht {
    pub(crate) fn new(actor_tx: mpsc::Sender<ActorMessage>) -> Self {
        Self { actor_tx }
    }

    async fn request<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T, DhtError>>) -> DhtCommand,
    ) -> Result<T, DhtError> {
        let (tx, rx) = oneshot::channel();
        self.actor_tx
            .send(ActorMessage::Dht(make(tx)))
            .await
            .map_err(|_| DhtError::Closed)?;
        rx.await.map_err(|_| DhtError::Closed)?
    }

    /// 引导路由表（需要先 `add_infrastructure_peer` 注入至少一个引导节点）。
    pub async fn bootstrap(&self) -> Result<(), DhtError> {
        self.request(|reply| DhtCommand::Bootstrap { reply }).await
    }

    /// 发布 record（`ttl` 为服务器侧过期时间）。
    pub async fn put(
        &self,
        key: DhtKey,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> Result<(), DhtError> {
        self.request(|reply| DhtCommand::Put {
            key,
            value,
            ttl,
            reply,
        })
        .await
    }

    /// 查询 record。
    pub async fn get(&self, key: DhtKey) -> Result<DhtRecord, DhtError> {
        self.request(|reply| DhtCommand::Get { key, reply }).await
    }

    /// 本地移除 record（停止 re-publish，网络副本等 TTL 过期）。
    pub async fn remove(&self, key: DhtKey) -> Result<(), DhtError> {
        self.request(|reply| DhtCommand::Remove { key, reply })
            .await
    }

    /// 宣告本节点为 `key` 的 provider。
    pub async fn provide(&self, key: DhtKey) -> Result<(), DhtError> {
        self.request(|reply| DhtCommand::Provide { key, reply })
            .await
    }

    /// 停止宣告。
    pub async fn stop_provide(&self, key: DhtKey) -> Result<(), DhtError> {
        self.request(|reply| DhtCommand::StopProvide { key, reply })
            .await
    }

    /// 查询 `key` 的 providers。
    pub async fn providers(&self, key: DhtKey) -> Result<Vec<NodeId>, DhtError> {
        self.request(|reply| DhtCommand::Providers { key, reply })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DhtKey 派生是 wire 契约：同输入必须永远同 key（跨版本查得到彼此的
    /// record），namespace 参与散列（share-code 与 online 空间隔离）。
    #[test]
    fn namespaced_key_is_deterministic_and_namespace_separated() {
        let a1 = DhtKey::namespaced("/swarmdrop/share-code/", b"123456");
        let a2 = DhtKey::namespaced("/swarmdrop/share-code/", b"123456");
        assert_eq!(a1, a2);

        let other_id = DhtKey::namespaced("/swarmdrop/share-code/", b"654321");
        assert_ne!(a1, other_id);

        let other_ns = DhtKey::namespaced("/swarmdrop/online/", b"123456");
        assert_ne!(a1, other_ns, "不同 namespace 必须落在不同 key 空间");

        // namespace/id 的边界参与散列（"ab"+"c" ≠ "a"+"bc"）
        assert_ne!(
            DhtKey::namespaced("ab", b"c"),
            DhtKey::namespaced("a", b"bc"),
        );
    }
}
