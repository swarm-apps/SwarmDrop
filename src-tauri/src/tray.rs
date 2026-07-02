//! 系统托盘
//!
//! 桌面常驻托盘：状态首行 + 6 项菜单 + 左键开窗。托盘在 `setup` 阶段创建，句柄
//! （`TrayIcon` 由 Tauri 内部持有 + 可变 `MenuItem` 存入 [`TrayState`]）长存——被
//! drop 会导致图标消失 / 无法动态改文案。
//!
//! 关闭语义（✕ → 缩盘 / 退出）由**前端** `onCloseRequested` 权威拦截，本模块只负责
//! 托盘本身与「暂停接收」的托盘侧切换；二者通过 [`ReceivingPausedChanged`] 事件保持同步。

use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuEvent, MenuItem};
#[cfg(not(target_os = "macos"))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};
use tauri_specta::Event as _;
use tracing::warn;

use crate::AppResult;
use crate::events::{ReceivingPausedChanged, TrayOpenReceiveFolder, TrayOpenSettings};
use crate::network::NetManagerState;
use swarmdrop_core::transfer::incoming::IncomingTransferRuntime;
use swarmdrop_core::transfer::manager::TransferManager;

// 菜单项 id
const ID_OPEN: &str = "tray-open";
const ID_PAUSE: &str = "tray-pause";
const ID_OPEN_FOLDER: &str = "tray-open-folder";
const ID_SETTINGS: &str = "tray-settings";
const ID_QUIT: &str = "tray-quit";

// 三态托盘图标：复用品牌 logo 的松鼠剪影（含面部负空间）。离线=灰调、在线=品牌金；
// 暂停=橙调且橡果盾位置换成暂停双杠——暂停态如果只换成琥珀色会和在线态的金
// 拉不开距离，所以用色相+形状双重区分。不用 macOS template 模式——template
// 图标会被系统强制去色成单色，没法用颜色区分三态。
const ICON_OFFLINE: &[u8] = include_bytes!("../icons/tray/offline.png");
const ICON_ONLINE: &[u8] = include_bytes!("../icons/tray/online.png");
const ICON_PAUSED: &[u8] = include_bytes!("../icons/tray/paused.png");

/// 托盘三态：离线 / 在线 / 暂停接收。状态首行文字与图标的唯一状态来源——以后
/// 加新消费者（比如 tooltip）时只在这里加方法，不要在别处再对 `(online, paused)`
/// 单独 match，否则新增状态容易漏改其中一处。
#[derive(Clone, Copy)]
enum TrayStatus {
    Offline,
    Online,
    Paused,
}

impl TrayStatus {
    fn from_flags(online: bool, paused: bool) -> Self {
        match (online, paused) {
            (false, _) => Self::Offline,
            (true, true) => Self::Paused,
            (true, false) => Self::Online,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Offline => "○ 未连接",
            Self::Paused => "⏸ 已暂停接收",
            Self::Online => "● 在线 · 可接收文件",
        }
    }

    fn icon_bytes(self) -> &'static [u8] {
        match self {
            Self::Offline => ICON_OFFLINE,
            Self::Paused => ICON_PAUSED,
            Self::Online => ICON_ONLINE,
        }
    }
}

/// 托盘可变句柄：状态首行 + 暂停项 + 图标本身。状态变化时改其文案 / 启用态 / 图标。
pub struct TrayState {
    status_item: MenuItem<Wry>,
    pause_item: MenuItem<Wry>,
    tray_icon: TrayIcon<Wry>,
}

/// 在 `setup` 阶段创建托盘。`MenuItem` 句柄存入 [`TrayState`] 长存。
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(
        app,
        "tray-status",
        TrayStatus::Offline.text(),
        false,
        None::<&str>,
    )?;
    let open_item = MenuItem::with_id(app, ID_OPEN, "打开 SwarmDrop", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, ID_PAUSE, pause_label(false), false, None::<&str>)?;
    let open_folder_item =
        MenuItem::with_id(app, ID_OPEN_FOLDER, "打开接收文件夹", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, ID_SETTINGS, "设置…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, ID_QUIT, "退出 SwarmDrop", true, None::<&str>)?;

    let menu = MenuBuilder::new(app)
        .item(&status_item)
        .item(&open_item)
        .item(&pause_item)
        .item(&open_folder_item)
        .separator()
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("SwarmDrop")
        .on_menu_event(on_menu_event);

    // 左键行为按平台惯例分化：
    // - macOS 菜单栏额外项：左键单击即弹菜单（系统原生行为，与 Tailscale / Dropbox 一致）。
    // - Windows / Linux 通知区：左键单击开窗、右键弹菜单。
    #[cfg(target_os = "macos")]
    {
        builder = builder.show_menu_on_left_click(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder
            .show_menu_on_left_click(false)
            .on_tray_icon_event(on_tray_icon_event);
    }

    // 初始态节点未启动 = 离线图标；状态文字层（status_item）已经反映同样的初始态。
    // 图标字节是编译期 include_bytes! 常量，解码失败只可能是资产本身损坏（构建期
    // bug），不是运行时瞬时状况，所以让它经 `?` 直接让托盘创建失败、快速暴露问题
    // ——而不是像 refresh_tray 那样静默保留旧图标。
    builder = builder.icon(Image::from_bytes(TrayStatus::Offline.icon_bytes())?);
    let tray_icon = builder.build(app)?;

    app.manage(TrayState {
        status_item,
        pause_item,
        tray_icon,
    });
    Ok(())
}

/// 刷新托盘状态首行 + 暂停项 + 图标（`online`=节点已启动，`paused`=已暂停接收）。
pub fn refresh_tray(app: &AppHandle, online: bool, paused: bool) {
    if let Some(state) = app.try_state::<TrayState>() {
        let status = TrayStatus::from_flags(online, paused);
        let _ = state.status_item.set_text(status.text());
        let _ = state.pause_item.set_text(pause_label(paused));
        // 节点未启动时无从暂停，禁用暂停项。
        let _ = state.pause_item.set_enabled(online);
        match Image::from_bytes(status.icon_bytes()) {
            Ok(icon) => {
                let _ = state.tray_icon.set_icon(Some(icon));
            }
            Err(e) => warn!("托盘图标解码失败，保留上一次的图标: {e}"),
        }
    }
}

/// 在锁内访问托管的 `TransferManager`（暂停态读写的唯一入口）。
/// 节点未启动返回 `node_not_started`。
async fn with_transfer<R>(app: &AppHandle, f: impl FnOnce(&TransferManager) -> R) -> AppResult<R> {
    let net = app
        .try_state::<NetManagerState>()
        .ok_or_else(crate::AppError::node_not_started)?;
    let guard = net.lock().await;
    let mgr = guard
        .as_ref()
        .ok_or_else(crate::AppError::node_not_started)?;
    Ok(f(mgr.transfer()))
}

/// 同步暂停态到托盘 + 广播 `ReceivingPausedChanged`（apply / toggle 共用）。
fn sync_paused_ui(app: &AppHandle, paused: bool) {
    refresh_tray(app, true, paused);
    let _ = ReceivingPausedChanged(paused).emit(app);
}

/// 设「暂停接收」为指定值并同步 UI。节点未启动返回 `node_not_started`。
pub async fn apply_receiving_paused(app: &AppHandle, paused: bool) -> AppResult<()> {
    with_transfer(app, |t| t.set_receiving_paused(paused)).await?;
    sync_paused_ui(app, paused);
    Ok(())
}

/// 切换「暂停接收」（单次持锁内读取并取反，避免双重加锁与 TOCTOU）。
pub async fn toggle_receiving_paused(app: &AppHandle) -> AppResult<()> {
    let next = with_transfer(app, |t| {
        let next = !t.is_receiving_paused();
        t.set_receiving_paused(next);
        next
    })
    .await?;
    sync_paused_ui(app, next);
    Ok(())
}

/// 读当前「暂停接收」状态（节点未启动视为未暂停）。
pub async fn current_receiving_paused(app: &AppHandle) -> bool {
    with_transfer(app, |t| t.is_receiving_paused())
        .await
        .unwrap_or(false)
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        ID_OPEN => show_main_window(app),
        ID_PAUSE => {
            // sync 回调里异步切换暂停态（单次持锁读+取反+写，见 toggle_receiving_paused）。
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = toggle_receiving_paused(&app).await {
                    warn!("托盘切换暂停接收失败: {e}");
                }
            });
        }
        ID_OPEN_FOLDER => {
            // 路径由前端 savePath 拥有 → 唤窗 + 发类型化事件给（缩盘后仍存活的）webview 执行。
            show_main_window(app);
            let _ = TrayOpenReceiveFolder.emit(app);
        }
        ID_SETTINGS => {
            show_main_window(app);
            let _ = TrayOpenSettings.emit(app);
        }
        ID_QUIT => app.exit(0),
        _ => {}
    }
}

/// 仅 Windows / Linux：左键单击打开/聚焦主窗口（右键由系统弹菜单）。
/// macOS 不注册此回调——左键由 `show_menu_on_left_click(true)` 直接弹菜单。
/// Linux 左键不可靠，菜单「打开」项兜底。
#[cfg(not(target_os = "macos"))]
fn on_tray_icon_event(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        show_main_window(tray.app_handle());
    }
}

/// 显示并聚焦主窗口（托盘「打开」/ 左键 / 单实例二次启动共用）。
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn pause_label(paused: bool) -> &'static str {
    if paused {
        "恢复接收"
    } else {
        "暂停接收"
    }
}
