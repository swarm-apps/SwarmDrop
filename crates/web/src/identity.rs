//! 身份与已配对设备的持久化。
//!
//! 身份：`SecretKey` 的 protobuf 编码经 hex 存储、启动恢复——节点身份必须稳定（circuit
//! 地址 / 分享码发布都绑 NodeId）。protobuf 编码与桌面/移动 keychain 存量同构。存储后端按
//! 环境双轨：Window 用 localStorage（同步、现状）；Worker 没有 localStorage，退到 OPFS 小
//! 文件（与落盘同一存储域，Worker 全自治、无需主线程注入身份）。
//!
//! 已配对设备：整份快照进 IndexedDB（[`crate::idb`] 的 `kv` store），`spawn()` 时注入
//! `start_node`。传输会话的持久化在 [`crate::store`]，两者不共用 store。

use swarmdrop_host::device::PairedDeviceInfo;
use swarmdrop_net::SecretKey;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::error::{WebError, WebResult};
use crate::idb;
use crate::opfs::{open_writable, opfs_file_handle};

const STORAGE_KEY: &str = "swarmdrop.identity.protobuf.hex";
const PAIRED_DEVICES_KEY: &str = "swarmdrop.pairedDevices.v1";
const OPFS_PATH: &str = ".swarmdrop/identity.protobuf.hex";

/// 恢复身份；缺失 / 损坏则生成新身份并写回。
pub async fn load_or_create() -> WebResult<SecretKey> {
    match crate::env::local_storage() {
        Some(storage) => load_or_create_local_storage(&storage),
        None => load_or_create_opfs().await,
    }
}

/// 恢复 Web 端已配对设备。优先 IndexedDB，旧 localStorage 数据只作为迁移兜底。
pub async fn load_paired_devices() -> WebResult<Vec<PairedDeviceInfo>> {
    if let Some(json) = idb::get_string(idb::KV_STORE, PAIRED_DEVICES_KEY).await? {
        return decode_paired_devices(&json);
    }
    let devices = load_legacy_paired_devices()?;
    if !devices.is_empty() {
        save_paired_devices(&devices).await?;
    }
    Ok(devices)
}

/// 写入 Web 端已配对设备快照，供刷新后注入 `start_node`。
pub async fn save_paired_devices(devices: &[PairedDeviceInfo]) -> WebResult<()> {
    let json = serde_json::to_string(devices)
        .map_err(|e| WebError::storage(format!("序列化已配对设备失败: {e}")))?;
    idb::put_string(idb::KV_STORE, PAIRED_DEVICES_KEY, &json).await
}

/// 幂等追加/更新单个配对设备。
pub async fn upsert_paired_device(device: PairedDeviceInfo) -> WebResult<Vec<PairedDeviceInfo>> {
    let mut devices = load_paired_devices().await?;
    if let Some(existing) = devices.iter_mut().find(|d| d.peer_id == device.peer_id) {
        *existing = device;
    } else {
        devices.push(device);
    }
    save_paired_devices(&devices).await?;
    Ok(devices)
}

fn decode_paired_devices(json: &str) -> WebResult<Vec<PairedDeviceInfo>> {
    serde_json::from_str(json).map_err(|e| WebError::storage(format!("解析已配对设备失败: {e}")))
}

fn load_legacy_paired_devices() -> WebResult<Vec<PairedDeviceInfo>> {
    let Some(storage) = crate::env::local_storage() else {
        return Ok(Vec::new());
    };
    let Some(json) = storage
        .get_item(PAIRED_DEVICES_KEY)
        .map_err(|_| WebError::storage("读取旧已配对设备失败"))?
    else {
        return Ok(Vec::new());
    };
    decode_paired_devices(&json)
}

/// Window 路径：localStorage 同步读写。
fn load_or_create_local_storage(storage: &web_sys::Storage) -> WebResult<SecretKey> {
    if let Ok(Some(hex)) = storage.get_item(STORAGE_KEY)
        && let Some(sk) = decode_secret(&hex)
    {
        return Ok(sk);
    }

    let sk = SecretKey::generate();
    storage
        .set_item(STORAGE_KEY, &encode_hex(&sk.to_protobuf()))
        .map_err(|_| WebError::storage("写入 localStorage 身份失败"))?;
    Ok(sk)
}

/// Worker 路径：OPFS 文件读写（读不到 / 解不开 → 生成并写回）。
async fn load_or_create_opfs() -> WebResult<SecretKey> {
    if let Some(sk) = read_opfs_identity().await {
        return Ok(sk);
    }

    let sk = SecretKey::generate();
    let hex = encode_hex(&sk.to_protobuf());
    let writable = open_writable(OPFS_PATH, false)
        .await
        .map_err(|e| WebError::storage(format!("开 OPFS 身份文件失败: {e}")))?;
    JsFuture::from(
        writable
            .write_with_str(&hex)
            .map_err(|_| WebError::storage("写 OPFS 身份失败"))?,
    )
    .await
    .map_err(|_| WebError::storage("写 OPFS 身份失败"))?;
    JsFuture::from(writable.close())
        .await
        .map_err(|_| WebError::storage("提交 OPFS 身份失败"))?;
    Ok(sk)
}

async fn read_opfs_identity() -> Option<SecretKey> {
    let handle = opfs_file_handle(OPFS_PATH, false).await.ok()?;
    let file: web_sys::File = JsFuture::from(handle.get_file())
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    let text = JsFuture::from(file.text()).await.ok()?.as_string()?;
    decode_secret(&text)
}

fn decode_secret(hex: &str) -> Option<SecretKey> {
    let bytes = decode_hex(hex.trim())?;
    SecretKey::from_protobuf(&bytes).ok()
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let bytes = s.as_bytes();
    (0..bytes.len() / 2)
        .map(|i| {
            let hi = (bytes[2 * i] as char).to_digit(16)?;
            let lo = (bytes[2 * i + 1] as char).to_digit(16)?;
            Some((hi * 16 + lo) as u8)
        })
        .collect()
}
