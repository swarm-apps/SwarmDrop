//! 身份持久化：`SecretKey` 的 protobuf 编码经 hex 存储，启动恢复。
//!
//! 范围内不做配对持久化，但节点身份必须稳定（circuit 地址 / 分享码发布都绑 NodeId），
//! 故最小地存一份密钥。protobuf 编码与桌面/移动 keychain 存量同构。
//!
//! 存储后端按环境双轨：Window 用 localStorage（同步、现状）；Worker 没有 localStorage，
//! 退到 OPFS 小文件（与落盘同一存储域，Worker 全自治、无需主线程注入身份）。

use js_sys::{Function, Promise, Reflect};
use swarmdrop_host::device::PairedDeviceInfo;
use swarmdrop_net::SecretKey;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Event, IdbDatabase, IdbObjectStore, IdbOpenDbRequest, IdbRequest};

use crate::error::{WebError, WebResult};
use crate::opfs::{open_writable, opfs_file_handle};

const STORAGE_KEY: &str = "swarmdrop.identity.protobuf.hex";
const PAIRED_DEVICES_KEY: &str = "swarmdrop.pairedDevices.v1";
const IDB_NAME: &str = "swarmdrop-web";
const IDB_VERSION: u32 = 1;
const IDB_STORE: &str = "kv";
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
    if let Some(json) = idb_get_string(PAIRED_DEVICES_KEY).await? {
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
    idb_put_string(PAIRED_DEVICES_KEY, &json).await
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

async fn idb_get_string(key: &str) -> WebResult<Option<String>> {
    let db = open_idb().await?;
    let store = idb_store(&db, web_sys::IdbTransactionMode::Readonly)?;
    let value = idb_request(store.get(&JsValue::from_str(key)).map_err(idb_js_error)?).await?;
    Ok(value.as_string())
}

async fn idb_put_string(key: &str, value: &str) -> WebResult<()> {
    let db = open_idb().await?;
    let store = idb_store(&db, web_sys::IdbTransactionMode::Readwrite)?;
    idb_request(
        store
            .put_with_key(&JsValue::from_str(value), &JsValue::from_str(key))
            .map_err(idb_js_error)?,
    )
    .await?;
    Ok(())
}

async fn open_idb() -> WebResult<IdbDatabase> {
    let factory = web_sys::window()
        .ok_or_else(|| WebError::storage("当前环境没有 Window，无法打开 IndexedDB"))?
        .indexed_db()
        .map_err(idb_js_error)?
        .ok_or_else(|| WebError::storage("当前浏览器不支持 IndexedDB"))?;
    let request = factory
        .open_with_u32(IDB_NAME, IDB_VERSION)
        .map_err(idb_js_error)?;
    install_upgrade_handler(&request);
    idb_request(request.unchecked_into::<IdbRequest>())
        .await?
        .dyn_into::<IdbDatabase>()
        .map_err(|_| WebError::storage("打开 IndexedDB 返回了非数据库对象"))
}

fn install_upgrade_handler(request: &IdbOpenDbRequest) {
    let upgrade_request = request.clone();
    let on_upgrade = Closure::wrap(Box::new(move |_event: Event| {
        if let Ok(db) = upgrade_request
            .result()
            .and_then(|value| value.dyn_into::<IdbDatabase>())
        {
            if !db.object_store_names().contains(IDB_STORE) {
                let _ = db.create_object_store(IDB_STORE);
            }
        }
    }) as Box<dyn FnMut(_)>);
    request.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));
    on_upgrade.forget();
}

fn idb_store(db: &IdbDatabase, mode: web_sys::IdbTransactionMode) -> WebResult<IdbObjectStore> {
    db.transaction_with_str_and_mode(IDB_STORE, mode)
        .and_then(|tx| tx.object_store(IDB_STORE))
        .map_err(idb_js_error)
}

async fn idb_request(request: IdbRequest) -> WebResult<JsValue> {
    let promise = Promise::new(&mut |resolve: Function, reject: Function| {
        let success_request = request.clone();
        let resolve_success = resolve.clone();
        let on_success = Closure::wrap(Box::new(move |_event: Event| {
            let result = success_request.result().unwrap_or(JsValue::UNDEFINED);
            let _ = resolve_success.call1(&JsValue::NULL, &result);
        }) as Box<dyn FnMut(_)>);

        let error_request = request.clone();
        let reject_error = reject.clone();
        let on_error = Closure::wrap(Box::new(move |_event: Event| {
            let error = error_request
                .error()
                .ok()
                .flatten()
                .map(JsValue::from)
                .unwrap_or_else(|| JsValue::from_str("IndexedDB 请求失败"));
            let _ = reject_error.call1(&JsValue::NULL, &error);
        }) as Box<dyn FnMut(_)>);

        request.set_onsuccess(Some(on_success.as_ref().unchecked_ref()));
        request.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_success.forget();
        on_error.forget();
    });
    JsFuture::from(promise).await.map_err(idb_js_error)
}

fn idb_js_error(value: JsValue) -> WebError {
    let message = value
        .dyn_ref::<web_sys::DomException>()
        .map(|e| e.message())
        .or_else(|| {
            Reflect::get(&value, &JsValue::from_str("message"))
                .ok()
                .and_then(|v| v.as_string())
        })
        .or_else(|| value.as_string())
        .unwrap_or_else(|| "IndexedDB 操作失败".to_string());
    WebError::storage(message)
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
