//! 分享码只读查询（复用 core 的 DHT record JSON 结构，不搬配对逻辑）。
//!
//! 与 `crates/core/src/pairing/{code,manager}.rs` 对齐：key = `DhtKey::namespaced(
//! "/swarmdrop/share-code/", code)`，record 是 JSON `ShareCodeRecord`（发布者 NodeId 取自
//! DHT record 的 `publisher`，可达地址取自 record 的 `listenAddrs`）。Web 壳只**读**分享码
//! 解析对端地址，不发布、不做配对握手。

use serde::Deserialize;
use swarmdrop_net::{Addr, DhtKey, Endpoint, NodeAddr};

use crate::error::{WebError, WebResult};

const SHARE_CODE_NS: &str = "/swarmdrop/share-code/";

/// core `ShareCodeRecord` JSON 的只读镜像（仅取查询所需字段；`os_info` 的 flatten 字段作为
/// 未知顶层键被 serde 忽略）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareCodeRecord {
    #[serde(default)]
    expires_at: i64,
    #[serde(default)]
    listen_addrs: Vec<Addr>,
}

/// 查分享码 → 解析出对端 [`NodeAddr`]（发布者 + 可达地址），并注册进本地地址簿。
pub async fn lookup(endpoint: &Endpoint, code: &str) -> WebResult<NodeAddr> {
    let dht = endpoint
        .dht()
        .ok_or_else(|| WebError::network("DHT 未启用"))?;
    let key = DhtKey::namespaced(SHARE_CODE_NS, code.trim().as_bytes());
    let record = dht
        .get(key)
        .await
        .map_err(|e| WebError::not_found(format!("分享码查询失败: {e}")))?;

    let publisher = record
        .publisher
        .ok_or_else(|| WebError::not_found("分享码记录无发布者"))?;
    let share: ShareCodeRecord = serde_json::from_slice(&record.value)
        .map_err(|e| WebError::not_found(format!("分享码记录解析失败: {e}")))?;

    if share.expires_at != 0 && share.expires_at < (js_sys::Date::now() / 1000.0) as i64 {
        return Err(WebError::not_found("分享码已过期"));
    }

    // 注册地址簿，后续 connect 能直接拨到发布者。
    if !share.listen_addrs.is_empty() {
        endpoint
            .add_addrs(publisher, share.listen_addrs.clone())
            .await
            .map_err(crate::error::js_err_to_web)?;
    }

    Ok(NodeAddr::with_addrs(publisher, share.listen_addrs))
}
