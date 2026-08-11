use swarmdrop_core::device::{DeviceName, PairedDeviceInfo};
use swarmdrop_core::host::{DeviceConfig, DeviceIdentityBytes};
use swarmdrop_net::SecretKey;
use tauri::{AppHandle, Manager, State};

use crate::AppResult;
use crate::host::event_bus::TauriEventBus;
use crate::network::NetManagerState;

#[derive(Debug, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IdentityState {
    pub keypair: Vec<u8>,
    pub device_id: String,
    pub paired_devices: Vec<PairedDeviceInfo>,
    pub created: bool,
}

/// 初始化设备身份（读身份存储，不存在则生成），不要求用户输入任何密码。
#[tauri::command]
#[specta::specta]
pub async fn initialize_identity(app: AppHandle) -> AppResult<IdentityState> {
    let provider = crate::host::keychain_provider(&app)?;
    let identity = swarmdrop_core::identity::load_or_create_identity(&*provider).await?;
    let store = crate::host::paired_device_store(&app)?;
    // 直接读端口：core 侧那个同名转发函数已删——它没有任何业务动作，只是把
    // `load_paired_devices` 再包一层。
    let paired_devices = store.load_paired_devices().await?;
    let device_id = identity.node_id.to_string();

    app.manage(identity.secret_key);

    Ok(IdentityState {
        keypair: identity.keypair_bytes,
        device_id,
        paired_devices,
        created: identity.created,
    })
}

/// 生成新的 Ed25519 密钥对（protobuf 编码）。
#[tauri::command]
#[specta::specta]
pub async fn generate_keypair() -> AppResult<Vec<u8>> {
    Ok(SecretKey::generate().to_protobuf())
}

/// 设备身份文件的绝对路径，供节点状态诊断展示。
///
/// 身份自 keychain 改为明文文件后，「存在哪」从一句不必关心的实现细节变成了用户
/// **需要**知道的事：备份、迁移设备、自行收紧权限都要先找得到它。诊断里只写
/// 「本机文件」而不给路径，等于把「系统钥匙串」换成另一个同样无法据此行动的说法。
#[tauri::command]
#[specta::specta]
pub async fn get_identity_file_path(app: AppHandle) -> AppResult<String> {
    Ok(crate::host::identity_file_path(&app)?.display().to_string())
}

/// 读取持久化的设备名（onboarding 完成前为 `None`）。
#[tauri::command]
#[specta::specta]
pub async fn get_device_name(app: AppHandle) -> AppResult<Option<String>> {
    Ok(crate::host::device_config(&app)?
        .load_device_name()
        .await
        .map(DeviceName::into_string))
}

/// 设置设备名：落盘 + 让已连接的对端立刻看到新名字。
///
/// 编排在 [`swarmdrop_core::device_name::rename_device`] 里（写盘 → 本机 `OsInfo` →
/// identify `agent_version` 逐连接下发 + 主动 push → 发 `DeviceRenamed` 事件）。
/// **节点不重启、连接不断、在途传输不中断。**
///
/// 「节点在不在跑」这件事由 core 的签名吸收：`guard.as_ref()` 本身就是
/// `Option<&NetManager>`，节点未启动（onboarding，或设置页早于 `start`）时它是 `None`，
/// core 走只落盘那条分支。宿主这侧因此不写 if/else。
///
/// 归一化（trim / 剥控制字符与 `;` / 40 char 截断 / 空则清空）全在
/// [`DeviceName::parse`] 里，本命令不做任何就地处理 —— 三端共用同一个入口。
/// `name = None`（或归一化后为空）清空，回退到系统 hostname。
///
/// **返回归一化后的结果**（清空时 `None`），前端直接拿它更新本地状态。命令手里本来就握着
/// `DeviceName::parse` 的产物，返回 `()` 只会逼调用方回读一次才知道真正落库的是什么。
/// Web 端 `rename_device` 早就是这个形态，本次三端统一。
#[tauri::command]
#[specta::specta]
pub async fn set_device_name(
    app: AppHandle,
    net: State<'_, NetManagerState>,
    name: Option<String>,
) -> AppResult<Option<String>> {
    let normalized = name.as_deref().and_then(DeviceName::parse);
    let events = app.state::<TauriEventBus>();
    let device_config = crate::host::device_config(&app)?;

    let guard = net.lock().await;
    swarmdrop_core::device_name::rename_device(
        normalized.clone(),
        &device_config,
        events.inner(),
        guard.as_ref(),
    )
    .await?;
    Ok(normalized.map(DeviceName::into_string))
}

/// 注册密钥对到 Tauri state，并在桌面端写入身份存储。
#[tauri::command]
#[specta::specta]
pub async fn register_keypair(app: AppHandle, keypair: Vec<u8>) -> AppResult<String> {
    let secret_key =
        SecretKey::from_protobuf(&keypair).map_err(|e| crate::AppError::identity(e.to_string()))?;
    let node_id = secret_key.node_id();

    let provider = crate::host::keychain_provider(&app)?;
    provider
        .save_identity(DeviceIdentityBytes {
            keypair: secret_key.to_protobuf(),
        })
        .await?;

    app.manage(secret_key);

    Ok(node_id.to_string())
}
