//! 设备名的 Web 持久化：[`DeviceConfig`] 端口的 IndexedDB 实现 + 设置页用的三个模块级导出。
//!
//! 与 [`crate::paired_devices`] 同一套路子：无状态零尺寸实现，直接读写
//! [`kv`](crate::idb::KV_STORE) store 下的一个键。桌面把设备名落在 `device_config.json`，
//! 浏览器没有文件系统语义可言，落 IndexedDB 是同一件事的浏览器写法。
//!
//! IndexedDB 的 `JsFuture` 是 `!Send`，用 `SendWrapper` 裹住以满足 `#[async_trait]` 的 Send
//! 约束（单线程 wasm 下跨线程 panic 永不触发，见 `dev-notes/knowledge/storage-abstraction.md`）。
//!
//! **load 不返回错误、save 返回错误**是端口契约（见 `DeviceConfig` 的 doc）：load 在
//! `start_node` 的启动路径上，一次 IndexedDB 打不开不该让节点起不来，降级回 hostname
//! （= 浏览器名）继续跑；save 只在用户点保存时发生，静默失败等于「改了名字，刷新后又变回去」。

use async_trait::async_trait;
use send_wrapper::SendWrapper;
use swarmdrop_host::device::DeviceName;
use swarmdrop_host::{AppResult, DeviceConfig};
use wasm_bindgen::prelude::*;

use crate::error::WebError;
use crate::idb;

/// IndexedDB `kv` store 里的 key。带版本后缀是这个库的既有约定（见
/// [`crate::paired_devices`]），改名要考虑存量库。
const DEVICE_NAME_KEY: &str = "swarmdrop.deviceName.v1";

/// 设备名的 Web 端口实现（无状态，直接读写 IndexedDB）。
#[derive(Debug, Default, Clone, Copy)]
pub struct IdbDeviceConfig;

#[async_trait]
impl DeviceConfig for IdbDeviceConfig {
    async fn load_device_name(&self) -> Option<DeviceName> {
        let raw = match SendWrapper::new(idb::get_string(idb::KV_STORE, DEVICE_NAME_KEY)).await {
            Ok(raw) => raw,
            Err(e) => {
                tracing::warn!("读取设备名失败，回退到浏览器名: {e:?}");
                return None;
            }
        };
        // 读出来的串再过一次归一化：库里的值可能被开发者工具手改过，而未归一化的名字
        // 会经 agent_version 的 `"; "` 分隔符注入出一个本机并不具备的 capability。
        raw.as_deref().and_then(DeviceName::parse)
    }

    async fn save_device_name(&self, name: Option<DeviceName>) -> AppResult<()> {
        let result = match name {
            Some(name) => {
                SendWrapper::new(async move {
                    idb::put_string(idb::KV_STORE, DEVICE_NAME_KEY, name.as_str()).await
                })
                .await
            }
            // `None` = 清空，回退到浏览器名。删键而不是写空串——空串在 load 侧同样会被
            // `DeviceName::parse` 判成 `None`，但留着一行垃圾没有意义。
            None => SendWrapper::new(idb::delete(idb::KV_STORE, DEVICE_NAME_KEY)).await,
        };
        result.map_err(Into::into)
    }
}

// === 模块级 wasm 导出 ===
//
// **刻意不挂 `WebNode`**：节点 spawn 失败（`status: "error"`）时设置页仍然可达，改名不该
// 被节点状态绑架。前端经 `node-runtime.ts` 的 `getModule()` 直接拿模块句柄调用，与
// `spawnNode()` 共用同一个记忆化 Promise，节点起不来不影响模块可用。
//
// 它们只落盘，不碰运行中的节点。**节点在跑时前端走的是
// [`WebNode::rename_device`](crate::node::WebNode::rename_device)**（写盘 + 本机 `OsInfo` +
// identify 逐连接下发，已连接的对端一个 RTT 内看到新名字）；这里是节点起不来时的那条退路，
// 新名字在下次 `spawn` 时由组合根从本端口读回。分支在 JS 侧，见 `node-runtime.ts`。

/// 当前持久化的设备名；未设过（或读失败 / 内容非法）返回 `undefined`。
#[wasm_bindgen]
pub async fn get_device_name() -> Option<String> {
    IdbDeviceConfig
        .load_device_name()
        .await
        .map(DeviceName::into_string)
}

/// 设置设备名；`null` / 空串 / 归一化后为空一律视为清空，回落到
/// [`default_device_name`]。入参经 `DeviceName::parse` 归一化（trim、剥控制字符与 `;`、
/// 截断到 40 个 char），UI 侧的 `maxLength` 只是提前拦一道。
#[wasm_bindgen]
pub async fn set_device_name(name: Option<String>) -> Result<(), JsValue> {
    let parsed = name.as_deref().and_then(DeviceName::parse);
    IdbDeviceConfig
        .save_device_name(parsed)
        .await
        .map_err(|e| WebError::from(e).to_js())
}

/// 未设设备名时对外展示的默认值（UA 派生的浏览器名，如 `"Chrome"`）。
///
/// 导出它而不是让前端再解析一次 UA：两份判定表迟早漂成「设置页 placeholder 写 Safari、
/// 对端看到 Browser」，既难发现又完全没有价值。这里返回的就是 `OsInfo::display_name()`
/// 在 `name` 缺省时回退到的那个 `hostname`。
#[wasm_bindgen]
pub fn default_device_name() -> String {
    crate::node::web_os_info().hostname
}
