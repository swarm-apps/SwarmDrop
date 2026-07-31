//! 已配对设备列表的 Web 持久化：[`PairedDeviceStore`] 端口的 IndexedDB 实现。
//!
//! **为什么这个文件不叫 `keychain.rs`。** 浏览器压根没有钥匙串——这份列表整份落在
//! IndexedDB 的 [`kv`](crate::idb::KV_STORE) store 里，没有任何操作系统密钥库参与；设备
//! 私钥走的是另一条完全不同的路（[`crate::identity`] 的 localStorage / OPFS）。端口拆分的
//! 依据正是「设备列表不是身份私钥」，用 keychain 命名会把刚拆开的两个概念在文件名层面
//! 重新粘回去。
//!
//! **本模块只有 load / save 两个动作，零业务判断。** upsert（保留既有信任策略）、改策略、
//! 移除都在 `swarmdrop_core::paired_devices`，三端共用同一份实现。Web 此前自己长了一套
//! 平行实现，其中的 upsert 是**整条替换**：对一台已配对设备再走一次邀请配对，会把用户设过的
//! `trust_level` / `receive_policy` 静默重置回默认——而 `receive_policy` 在 Web 上同样是被
//! `swarmdrop_transfer::policy` 真正裁决的。那套实现已随本端口一并删除。
//!
//! IndexedDB 的 `JsFuture` 是 `!Send`，用 `SendWrapper` 裹住以满足 `#[async_trait]` 的 Send
//! 约束（单线程 wasm 下跨线程 panic 永不触发，见 `dev-notes/knowledge/storage-abstraction.md`）。

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use swarmdrop_host::device::PairedDeviceInfo;
use swarmdrop_host::{AppError, AppResult, PairedDeviceStore};

use crate::error::{WebError, WebResult};
use crate::idb;

/// IndexedDB `kv` store 里的 key。**同时也是早期 localStorage 版本的 key**（见
/// [`load_legacy`]），改名要考虑存量库。
const PAIRED_DEVICES_KEY: &str = "swarmdrop.pairedDevices.v1";

/// 已配对设备列表的 Web 端口实现（无状态，直接读写 IndexedDB）。
#[derive(Debug, Default, Clone, Copy)]
pub struct WebPairedDeviceStore;

#[async_trait]
impl PairedDeviceStore for WebPairedDeviceStore {
    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>> {
        SendWrapper::new(load()).await.map_err(AppError::from)
    }

    async fn save_paired_devices(&self, devices: &[PairedDeviceInfo]) -> AppResult<()> {
        SendWrapper::new(save(devices))
            .await
            .map_err(AppError::from)
    }
}

/// 读取整份快照。优先 IndexedDB，旧 localStorage 数据只作为迁移兜底。
async fn load() -> WebResult<Vec<PairedDeviceInfo>> {
    if let Some(json) = idb::get_string(idb::KV_STORE, PAIRED_DEVICES_KEY).await? {
        return decode(&json);
    }
    let devices = load_legacy()?;
    if !devices.is_empty() {
        save(&devices).await?;
    }
    Ok(devices)
}

/// 整份覆写快照。
async fn save(devices: &[PairedDeviceInfo]) -> WebResult<()> {
    let json = serde_json::to_string(devices)
        .map_err(|e| WebError::storage(format!("序列化已配对设备失败: {e}")))?;
    idb::put_string(idb::KV_STORE, PAIRED_DEVICES_KEY, &json).await
}

fn decode(json: &str) -> WebResult<Vec<PairedDeviceInfo>> {
    serde_json::from_str(json).map_err(|e| WebError::storage(format!("解析已配对设备失败: {e}")))
}

/// 迁移兜底：更早的版本把这份快照写在 localStorage 上，读到即回写进 IndexedDB。
fn load_legacy() -> WebResult<Vec<PairedDeviceInfo>> {
    let Some(storage) = crate::env::local_storage() else {
        return Ok(Vec::new());
    };
    let Some(json) = storage
        .get_item(PAIRED_DEVICES_KEY)
        .map_err(|_| WebError::storage("读取旧已配对设备失败"))?
    else {
        return Ok(Vec::new());
    };
    decode(&json)
}
