//! [`AddressLookup`]：可插拔地址解析扩展点。
//!
//! 与 iroh 的本质差异：libp2p 的 mdns/kad 是 Swarm 内 behaviour（push 源，
//! 发现结果直接进 actor 的地址簿），不走本 trait；本 trait 承接 **pull 源**
//! ——`connect(NodeId)` 无候选地址时并发查询（在线宣告 record、rendezvous、
//! 静态配置等）。
//!
//! 「ergonomic trait + Builder 回填」（iroh address_lookup.rs 同款）：
//! lookup 可能需要 Endpoint 本身（如基于 DHT 的在线宣告 lookup），
//! Builder 存 `Box<dyn AddressLookupBuilder>`，bind 完成后回填构造。

use futures::stream::BoxStream;
use swarmdrop_net_base::{Addr, NodeId};

use crate::endpoint::{AddrsInfo, Endpoint};

/// 地址解析失败。
#[derive(Debug, thiserror::Error)]
#[error("lookup failed: {0}")]
pub struct LookupError(pub String);

/// 本机节点信息（发布型 lookup 的输入）。
#[derive(Debug, Clone)]
pub struct LocalNodeInfo {
    pub node_id: NodeId,
    pub addrs: AddrsInfo,
}

/// 地址解析扩展点。
pub trait AddressLookup: Send + Sync + std::fmt::Debug + 'static {
    /// 解析对端候选地址。返回 `None` 表示不认识该节点；
    /// 流允许多轮渐进产出（先缓存后网络）。
    fn resolve(&self, node: NodeId) -> Option<BoxStream<'static, Result<Vec<Addr>, LookupError>>>;

    /// 本机地址变化回调（发布型 lookup 用；默认空——只读服务不用管）。
    fn publish(&self, _info: &LocalNodeInfo) {}
}

/// lookup 的延迟构造（bind 后回填 `&Endpoint`，解决鸡生蛋）。
pub trait AddressLookupBuilder: Send + Sync + 'static {
    fn into_address_lookup(
        self: Box<Self>,
        endpoint: &Endpoint,
    ) -> Result<Box<dyn AddressLookup>, LookupError>;
}

/// 不需要 Endpoint 的 lookup 自动就是 Builder。
impl<T: AddressLookup> AddressLookupBuilder for T {
    fn into_address_lookup(
        self: Box<Self>,
        _endpoint: &Endpoint,
    ) -> Result<Box<dyn AddressLookup>, LookupError> {
        Ok(self)
    }
}

/// 闭包形式的 Builder（`|endpoint| MyLookup::new(endpoint.dht()...)`）。
pub struct LookupBuilderFn<F>(pub F);

impl<F> AddressLookupBuilder for LookupBuilderFn<F>
where
    F: FnOnce(&Endpoint) -> Result<Box<dyn AddressLookup>, LookupError> + Send + Sync + 'static,
{
    fn into_address_lookup(
        self: Box<Self>,
        endpoint: &Endpoint,
    ) -> Result<Box<dyn AddressLookup>, LookupError> {
        (self.0)(endpoint)
    }
}

/// 静态地址表 lookup（手动配置 / 测试用）。
#[derive(Debug, Default)]
pub struct StaticLookup {
    entries: std::collections::HashMap<NodeId, Vec<Addr>>,
}

impl StaticLookup {
    pub fn new(entries: impl IntoIterator<Item = (NodeId, Vec<Addr>)>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }
}

impl AddressLookup for StaticLookup {
    fn resolve(&self, node: NodeId) -> Option<BoxStream<'static, Result<Vec<Addr>, LookupError>>> {
        let addrs = self.entries.get(&node)?.clone();
        Some(Box::pin(futures::stream::once(async move { Ok(addrs) })))
    }
}

/// 并发解析：所有 lookup 同时查，任一产出即注入；全部结束仍无地址则失败。
///
/// 返回收集到的全部地址（去重）。
pub(crate) async fn resolve_all(lookups: &[Box<dyn AddressLookup>], node: NodeId) -> Vec<Addr> {
    use futures::StreamExt;

    let streams: Vec<_> = lookups.iter().filter_map(|l| l.resolve(node)).collect();
    if streams.is_empty() {
        return Vec::new();
    }
    let mut merged = futures::stream::select_all(streams);
    let mut found = Vec::new();
    while let Some(item) = merged.next().await {
        match item {
            Ok(addrs) => {
                for a in addrs {
                    if !found.contains(&a) {
                        found.push(a);
                    }
                }
            }
            Err(e) => tracing::debug!(error = %e, "address lookup source failed"),
        }
    }
    found
}
