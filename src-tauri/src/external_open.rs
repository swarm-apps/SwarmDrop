//! 外部「用 SwarmDrop 打开」入口
//!
//! 处理操作系统「打开方式 / Open With」送达的文件与文件夹路径，归一化后交给
//! 前端发起快捷发送（share-target 反向流）。三平台入口各不相同，但都汇入本模块：
//! - macOS：`RunEvent::Opened { urls }` → [`handle_opened`]（见 [`crate::run`]）
//! - Windows / Linux 冷启动：`std::env::args()` → [`handle_launch_args`]（见 [`crate::setup`]）
//! - Windows / Linux 已运行：single-instance 回调 argv → [`handle_second_instance`]
//!
//! 三条路径最终都汇入 [`ingest_paths`]：短去抖窗口内合并 → 已就绪则 emit
//! [`ExternalFileOpen`]，未就绪则缓冲，待前端根处理器 mount 时经 [`take_pending`] 取走
//! （解决冷启动竞态：事件可能早于前端订阅）。
//!
//! 平台策略（「macOS 走事件、其余走 argv」「各平台注册机制不同」）一律封装在本模块内，
//! 调用方（`lib.rs` / `setup.rs`）保持无 `cfg` 的统一调用。

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::AppHandle;
use tauri_specta::Event;

use crate::events::ExternalFileOpen;

/// 去抖合并窗口：一次「打开多个文件」或系统为每个文件各拉一个实例时，
/// 落在这个窗口内的路径合并成一次事件，避免前端连开多屏。
const COALESCE_WINDOW: Duration = Duration::from_millis(200);

#[derive(Default)]
struct Inner {
    /// 前端根处理器是否已挂载并拉取过一次；此后新路径走事件而非缓冲。
    frontend_ready: bool,
    /// 累积待发（已就绪）或待取（未就绪）的路径。
    buffer: Vec<PathBuf>,
    /// 是否已排定一次去抖 flush，避免重复 spawn。
    flush_scheduled: bool,
}

/// 进程内缓冲，用**全局** `OnceLock` 而非 Tauri 托管状态。
///
/// 关键：macOS 冷启动经「打开方式」时，`RunEvent::Opened` 可能早于 `setup()`
/// `app.manage(...)` 就到达；若那时访问托管状态，`app.state()` 会 panic，而该回调
/// 处于 ObjC `extern "C"` 边界、panic 不可 unwind → 直接 abort（见此前崩溃报告）。
/// 全局缓冲不依赖 setup 时序，冷启动路径也无需触碰 `AppHandle`。
fn pending() -> &'static Mutex<Inner> {
    static PENDING: OnceLock<Mutex<Inner>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(Inner::default()))
}

/// 前端根处理器 mount 时调用：标记就绪并取走冷启动期间缓冲的路径（取走即清空，保证同一批
/// 不被事件与缓冲双重处理）。命令薄壳见 [`crate::commands::take_pending_external_open`]。
pub fn take_pending() -> Vec<String> {
    let mut inner = pending().lock().unwrap();
    inner.frontend_ready = true;
    std::mem::take(&mut inner.buffer)
        .into_iter()
        .map(path_to_string)
        .collect()
}

/// 接收一批外部打开的目标路径。只保留真实存在的文件/目录；已就绪则去抖后 emit
/// [`ExternalFileOpen`]，未就绪则缓冲留待 [`take_pending`]。
pub fn ingest_paths(app: &AppHandle, paths: Vec<PathBuf>) {
    let paths: Vec<PathBuf> = paths.into_iter().filter(|p| p.exists()).collect();
    if paths.is_empty() {
        return;
    }
    tracing::debug!(count = paths.len(), "external open: ingest paths");

    let (ready, schedule) = {
        let mut inner = pending().lock().unwrap();
        inner.buffer.extend(paths);
        // 未就绪：只缓冲，等前端 mount 时一并取走。
        // 已就绪且尚未排定 flush：排定一次去抖 flush。
        let schedule = inner.frontend_ready && !inner.flush_scheduled;
        if schedule {
            inner.flush_scheduled = true;
        }
        (inner.frontend_ready, schedule)
    };

    // 仅在前端已就绪（app 已完整运行、可能缩在托盘）时唤出主窗口，否则用户点了「打开方式」
    // 看不到选设备屏 / 提示。冷启动时前端未就绪、窗口本就默认显示，且此刻在 macOS Opened
    // 早期路径调 AppKit 窗口操作有风险，故不在此处唤窗。
    if ready {
        crate::tray::show_main_window(app);
    }

    if schedule {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(COALESCE_WINDOW).await;
            let batch = {
                let mut inner = pending().lock().unwrap();
                inner.flush_scheduled = false;
                std::mem::take(&mut inner.buffer)
            };
            if batch.is_empty() {
                return;
            }
            let payload = ExternalFileOpen {
                paths: batch.into_iter().map(path_to_string).collect(),
            };
            if let Err(e) = payload.emit(&app) {
                tracing::warn!("external open: failed to emit event: {e}");
            }
        });
    }
}

/// macOS：处理 `RunEvent::Opened` 的文件 URL（归一化为本地路径后走 [`ingest_paths`]）。
/// 其他平台无此入口。
#[cfg(target_os = "macos")]
pub fn handle_opened(app: &AppHandle, urls: &[url::Url]) {
    let paths = urls.iter().filter_map(|u| u.to_file_path().ok()).collect();
    ingest_paths(app, paths);
}

/// 冷启动：从进程启动参数解析被打开的路径。macOS 走 [`handle_opened`]，此处为 no-op。
pub fn handle_launch_args(app: &AppHandle) {
    #[cfg(not(target_os = "macos"))]
    ingest_from_args(app, std::env::args());
    #[cfg(target_os = "macos")]
    let _ = app;
}

/// 第二实例（已运行时再次「打开」）：从 single-instance argv 解析路径。macOS 为 no-op。
pub fn handle_second_instance(app: &AppHandle, args: Vec<String>) {
    #[cfg(not(target_os = "macos"))]
    ingest_from_args(app, args);
    #[cfg(target_os = "macos")]
    let _ = (app, args);
}

/// 从命令行参数解析出存在的文件/目录路径并 ingest（跳过程序名与 flag，如 macOS 的 `-psn_*`）。
#[cfg(not(target_os = "macos"))]
fn ingest_from_args<I: IntoIterator<Item = String>>(app: &AppHandle, args: I) {
    let paths: Vec<PathBuf> = args
        .into_iter()
        .skip(1) // 程序名
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .collect();
    if !paths.is_empty() {
        ingest_paths(app, paths);
    }
}

fn path_to_string(p: PathBuf) -> String {
    p.to_string_lossy().into_owned()
}

// ============ OS 「打开方式」自注册 ============

/// 注册系统右键入口。「打开方式」子菜单 vs 顶层菜单项是两种不同机制，分开处理：
/// - **macOS / Linux 的「打开方式」**：由 `tauri.conf.json` 的 `bundle.fileAssociations`
///   （按扩展名）在打包时生成——macOS 通用 `public.data` 会被归属抑制，只能按扩展名列举。
/// - **Windows 顶层右键菜单**（像「通过 Code 打开」「通过 QQ 发送」那样直接显示）：本函数写
///   HKCU 注册表 shell verb（`*\shell` 任意文件 + `Directory\shell` 文件夹），比「打开方式」
///   更直接、且覆盖**所有**文件（Windows 无 macOS 那种 UTI 抑制）。
/// - **Linux 文件夹**：本函数写 `MimeType=inode/directory` 的 `.desktop`（fileAssociations
///   按扩展名表达不了目录）。
/// - **macOS 顶层菜单 / 文件夹**：需原生 Finder Sync Extension（Tauri 不脚手架），本轮不做。
///
/// 放后台线程、不占启动关键路径、非致命（失败仅告警）；各实现幂等短路（已指向当前 exe 则跳过）。
pub fn register_open_with() {
    #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
    std::thread::spawn(|| match register_platform() {
        Ok(()) => tracing::debug!("external open: registered OS open-with handler"),
        Err(e) => tracing::warn!("external open: failed to register open-with handler: {e}"),
    });
}

#[cfg(target_os = "windows")]
fn register_platform() -> std::io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let exe = std::env::current_exe()?;
    let exe = exe.to_string_lossy().into_owned();
    let command = format!("\"{exe}\" \"%1\"");
    let label = "Send with SwarmDrop".to_string();
    let icon = format!("\"{exe}\"");

    // 顶层右键菜单项（像「通过 Code 打开」「通过 QQ 发送」那样直接显示，而非埋进「打开方式」）：
    // `*\shell` = 任意文件、`Directory\shell` = 文件夹。用简单 command verb（非 COM 扩展），
    // Win11 新版菜单也会直接展示、不落到「显示更多选项」。HKCU 无需管理员。
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for base in [
        r"Software\Classes\*\shell\SwarmDrop",
        r"Software\Classes\Directory\shell\SwarmDrop",
    ] {
        let cmd_path = format!(r"{base}\command");
        // 幂等短路：command 键已是目标值就跳过整组写入。
        let already = hkcu
            .open_subkey(&cmd_path)
            .and_then(|k| k.get_value::<String, _>(""))
            .map(|v| v == command)
            .unwrap_or(false);
        if already {
            continue;
        }
        let (verb, _) = hkcu.create_subkey(base)?;
        verb.set_value("", &label)?;
        verb.set_value("Icon", &icon)?;
        let (cmd, _) = hkcu.create_subkey(&cmd_path)?;
        cmd.set_value("", &command)?;
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn register_platform() -> std::io::Result<()> {
    use std::io::Write;

    let exe = std::env::current_exe()?;
    let exe = exe.to_string_lossy();
    let home = std::env::var_os("HOME")
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME not set"))?;
    let apps_dir = std::path::Path::new(&home).join(".local/share/applications");
    let desktop_path = apps_dir.join("swarmdrop-open-with-folder.desktop");

    // 只补文件夹（inode/directory）——文件的 MimeType 由 Tauri fileAssociations 生成的
    // .desktop 承载。NoDisplay=true：不在应用菜单里另立入口，但仍作为文件管理器「打开方式」
    // 候选；若某桌面环境下因此从「打开方式」消失，去掉该行即可。
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Send with SwarmDrop\n\
         Exec=\"{exe}\" %F\n\
         NoDisplay=true\n\
         MimeType=inode/directory;\n"
    );

    // 幂等短路：.desktop 已存在且内容一致 → 跳过写入 + 跳过 update-desktop-database。
    if std::fs::read_to_string(&desktop_path).is_ok_and(|c| c == content) {
        return Ok(());
    }

    std::fs::create_dir_all(&apps_dir)?;
    std::fs::File::create(&desktop_path)?.write_all(content.as_bytes())?;

    // 刷新 MIME 缓存（best-effort、不等待子进程退出：缺该工具或失败都不影响注册本身）。
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&apps_dir)
        .spawn();
    Ok(())
}
