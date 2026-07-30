//! 外部「用 SwarmDrop 打开」入口
//!
//! 宿主层的**外部入口分发器**：把操作系统送来的东西归一化后交给前端。两类负载共用同一套
//! 机制，只在最后一步分流（openspec: pair-deep-link design D2）：
//!
//! | 负载 | 来源 | 去处 |
//! |---|---|---|
//! | 文件 / 文件夹路径 | 「打开方式 / Open With」 | [`ExternalFileOpen`] → 快捷发送 |
//! | 配对邀请链接 | `swarmdrop:` 深链（**单冒号，无 `//`**，见 [`dispatch_url`]） | [`ExternalPairInvite`] → 配对确认 |
//!
//! **为什么共用而不是各写一套**：深链遇到的问题与「打开方式」逐条相同 —— 冷启动时事件早于
//! 前端 mount（点链接拉起 App 是典型冷启动）、macOS 的 ObjC 回调边界 panic 不可 unwind、
//! Windows/Linux 已运行时走 single-instance argv。这些机制在下面只有一份。
//!
//! 三平台入口各不相同，但都汇入本模块：
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

use crate::events::{ExternalFileOpen, ExternalPairInvite};

/// 去抖合并窗口：一次「打开多个文件」或系统为每个文件各拉一个实例时，
/// 落在这个窗口内的路径合并成一次事件，避免前端连开多屏。
const COALESCE_WINDOW: Duration = Duration::from_millis(200);

#[derive(Default)]
struct Inner {
    /// 前端根处理器是否已挂载并拉取过一次；此后新负载走事件而非缓冲。
    frontend_ready: bool,
    /// 累积待发（已就绪）或待取（未就绪）的路径。
    buffer: Vec<PathBuf>,
    /// 待发/待取的配对邀请链接。
    ///
    /// **只留最后一条**，与路径的「合并成一批」刻意不同：一次能打开多个文件是常态，
    /// 而同时收到两条邀请是异常（用户一次只配一台设备），把它们攒成数组只会让前端
    /// 面对一个没有正确答案的选择。最后一条 = 用户最近点的那个。
    invite: Option<String>,
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

/// 前端根处理器 mount 时调用：标记就绪并**一次取走两类负载**（取走即清空，保证同一批不被
/// 事件与缓冲双重处理）。命令薄壳见 [`crate::commands::take_pending_external_open`]。
///
/// 两类负载必须由**同一次调用**取走：`frontend_ready` 是共享标记，拆成两个命令的话第一个
/// 调用就把标记置位了，第二类负载在那之后只走事件 —— 而此刻前端还没订阅完，正好丢在缝里。
pub fn take_pending() -> PendingExternalOpen {
    let mut inner = pending().lock().unwrap();
    inner.frontend_ready = true;
    PendingExternalOpen {
        paths: std::mem::take(&mut inner.buffer)
            .into_iter()
            .map(path_to_string)
            .collect(),
        invite: inner.invite.take(),
    }
}

/// 冷启动期间缓冲的外部入口负载（一次取走，见 [`take_pending`]）。
#[derive(Debug, Default, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PendingExternalOpen {
    /// 「打开方式」送达的路径（可能多个）。
    pub paths: Vec<String>,
    /// 深链送达的配对邀请链接（只留最后一条，见 [`Inner::invite`]）。
    pub invite: Option<String>,
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
        spawn_flush(app);
    }
}

/// 接收一条外部送达的配对邀请链接（深链）。就绪则去抖后 emit [`ExternalPairInvite`]，
/// 未就绪则缓冲留待 [`take_pending`]。
///
/// **不在此处解码验签**：那是 core 的事（`decode_pair_invite`），宿主层只负责把文本递进去。
/// 前端拿到后照常走「确认卡 → 用户确认」的安全闸，与扫码/粘贴同一条路。
pub fn ingest_invite(app: &AppHandle, invite: String) {
    if invite.trim().is_empty() {
        return;
    }
    tracing::debug!("external open: ingest invite link");

    let (ready, schedule) = {
        let mut inner = pending().lock().unwrap();
        inner.invite = Some(invite);
        let schedule = inner.frontend_ready && !inner.flush_scheduled;
        if schedule {
            inner.flush_scheduled = true;
        }
        (inner.frontend_ready, schedule)
    };

    // 与路径同理：已就绪（app 可能缩在托盘）才唤窗，冷启动路径不碰 AppKit。
    if ready {
        crate::tray::show_main_window(app);
    }
    if schedule {
        spawn_flush(app);
    }
}

/// 去抖 flush：一个窗口内攒下的两类负载各自 emit（都空则什么也不做）。
fn spawn_flush(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(COALESCE_WINDOW).await;
        let (paths, invite) = {
            let mut inner = pending().lock().unwrap();
            inner.flush_scheduled = false;
            (std::mem::take(&mut inner.buffer), inner.invite.take())
        };

        if !paths.is_empty() {
            let payload = ExternalFileOpen {
                paths: paths.into_iter().map(path_to_string).collect(),
            };
            if let Err(e) = payload.emit(&app) {
                tracing::warn!("external open: failed to emit file event: {e}");
            }
        }
        if let Some(invite) = invite
            && let Err(e) = (ExternalPairInvite { invite }).emit(&app)
        {
            tracing::warn!("external open: failed to emit invite event: {e}");
        }
    });
}

/// 把一个外部 URL 分流到对应入口。
///
/// `file://` → 路径（「打开方式」），`swarmdrop:` → 配对邀请（深链）。
/// 其余 scheme 静默忽略并记一条 debug —— 系统偶尔会送来我们没注册的东西，不该报错。
///
/// **深链的邀请文本取整个 URL 原样**，而不是抠出某一段：canonical 邀请链接是
/// `https://swarmapp.cn/p/#<payload>`，深链形态把它挂在 `swarmdrop:` 后面，而 core 的
/// `PairInvite::decode` 本来就能从任意文本里定位并提取 —— 少一层解析就少一处能漂移的地方。
///
/// ⚠️ **必须是 `swarmdrop:` 单冒号，不能写 `swarmdrop://`。** 加了 `//` 之后 `https:` 会被
/// 当成 authority 解析（host = `https`，端口为空），而 WHATWG 序列化会丢掉空端口的冒号：
///
/// ```text
/// swarmdrop://https://swarmapp.cn/p/#AB  --url::Url-->  swarmdrop://https//swarmapp.cn/p/#AB
///                                                                        ^ 冒号没了
/// ```
///
/// canonical 前缀因此匹配不上，`decode` 返回 `Kind`。而这**只在 macOS 上暴露**（那条路径
/// 经 `RunEvent::Opened` 拿到已被解析过的 `url::Url`）；Windows / Linux 走 argv 原样字符串，
/// 两种形态都能认 —— 典型的平台分叉静默失败。`tests::deep_link_contract` 钉死这一点，
/// **落地页与移动端生成深链时也必须用单冒号形态**。
fn dispatch_url(app: &AppHandle, url: &url::Url) {
    match url.scheme() {
        "file" => {
            if let Ok(path) = url.to_file_path() {
                ingest_paths(app, vec![path]);
            }
        }
        DEEP_LINK_SCHEME => ingest_invite(app, url.as_str().to_owned()),
        other => tracing::debug!("external open: 忽略未注册的 scheme: {other}"),
    }
}

/// 深链 scheme。与 `tauri.conf.json` 的 `plugins.deep-link.desktop.schemes`
/// 以及移动端 `app.json` 的 `scheme` 必须一致。
pub const DEEP_LINK_SCHEME: &str = "swarmdrop";

/// macOS：处理 `RunEvent::Opened` 送来的 URL。
///
/// **「打开方式」与深链在 macOS 上是同一个事件** —— 系统给的都是 URL，只是 scheme 不同，
/// 所以这里按 scheme 分流（见 [`dispatch_url`]）。其他平台无此入口。
#[cfg(target_os = "macos")]
pub fn handle_opened(app: &AppHandle, urls: &[url::Url]) {
    for url in urls {
        dispatch_url(app, url);
    }
}

/// 冷启动：从进程启动参数解析被打开的路径 / 深链。macOS 走 [`handle_opened`]，此处为 no-op。
pub fn handle_launch_args(app: &AppHandle) {
    #[cfg(not(target_os = "macos"))]
    ingest_from_args(app, std::env::args());
    #[cfg(target_os = "macos")]
    let _ = app;
}

/// 第二实例（已运行时再次「打开」）：从 single-instance argv 解析。macOS 为 no-op。
pub fn handle_second_instance(app: &AppHandle, args: Vec<String>) {
    #[cfg(not(target_os = "macos"))]
    ingest_from_args(app, args);
    #[cfg(target_os = "macos")]
    let _ = (app, args);
}

/// 从命令行参数解析外部入口并 ingest（跳过程序名与 flag，如 macOS 的 `-psn_*`）。
///
/// Windows / Linux 上深链是作为**一个 argv 项**传进来的（`swarmdrop://…`），与被打开的
/// 文件路径混在同一串参数里，所以这里要先按「是不是我们的 scheme」分流，再把剩下的当路径。
#[cfg(not(target_os = "macos"))]
fn ingest_from_args<I: IntoIterator<Item = String>>(app: &AppHandle, args: I) {
    let scheme_prefix = format!("{DEEP_LINK_SCHEME}:");
    let mut paths: Vec<PathBuf> = Vec::new();
    for arg in args.into_iter().skip(1 /* 程序名 */) {
        if arg.starts_with('-') {
            continue;
        }
        if arg
            .get(..scheme_prefix.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&scheme_prefix))
        {
            ingest_invite(app, arg);
            continue;
        }
        paths.push(PathBuf::from(arg));
    }
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

#[cfg(test)]
mod tests {
    use super::DEEP_LINK_SCHEME;
    use swarmdrop_invite::{InviteParseError, PairInvite};

    /// 一条形态正确、payload 是垃圾的 canonical 链接。
    ///
    /// 只用来验「前缀有没有被找到」：`Kind` = 没找到（不是邀请链接），其它任何错误都说明
    /// 前缀已定位、解析进到了 payload。所以本测试不需要真签名，也就不需要 SecretKey。
    const FAKE_CANONICAL: &str = "https://swarmapp.cn/p/#AAAAAAAA";

    fn prefix_found(text: &str) -> bool {
        !matches!(PairInvite::decode(text), Err(InviteParseError::Kind))
    }

    /// **深链形态的契约**：`swarmdrop:` + canonical 链接，单冒号、无 `//`。
    ///
    /// 这条测试存在的理由是一个平台分叉的静默失败（详见 [`super::dispatch_url`] 的文档）：
    /// macOS 的 `RunEvent::Opened` 给的是已解析的 `url::Url`，`//` 形态经序列化会丢掉
    /// `https:` 的冒号，前缀就匹配不上了。Windows / Linux 走 argv 原样字符串，察觉不到。
    ///
    /// 落地页的「在 App 中打开」按钮与 Android intent 也照这个形态生成。
    #[test]
    fn deep_link_contract() {
        let good = format!("{DEEP_LINK_SCHEME}:{FAKE_CANONICAL}");
        let bad = format!("{DEEP_LINK_SCHEME}://{FAKE_CANONICAL}");

        // 1) 两种形态都能被 url crate 接受 —— 所以坏形态不会在解析阶段就被挡下，
        //    它会一路走到 decode 才失败，这正是它难被发现的原因。
        let good_url = url::Url::parse(&good).expect("单冒号形态应当可解析");
        let bad_url = url::Url::parse(&bad).expect("双斜杠形态也可解析（问题不在这里）");
        assert_eq!(good_url.scheme(), DEEP_LINK_SCHEME);
        assert_eq!(bad_url.scheme(), DEEP_LINK_SCHEME);

        // 2) 但只有单冒号形态在 URL 往返后仍保留 canonical 前缀。
        assert!(
            prefix_found(good_url.as_str()),
            "单冒号形态往返后必须仍能定位 canonical 前缀，实际得到: {}",
            good_url.as_str()
        );
        assert!(
            !prefix_found(bad_url.as_str()),
            "双斜杠形态往返后前缀会被破坏（host=https + 空端口，冒号被丢弃）。\
             若这条断言红了，说明 url crate 改了序列化行为 —— 那是好事，\
             但要回去把 dispatch_url 的文档和落地页/Android 的生成形态一起复核。实际得到: {}",
            bad_url.as_str()
        );

        // 3) 原样字符串（Windows / Linux 的 argv 路径）两种形态都能认 —— 平台分叉就在这。
        assert!(
            prefix_found(&good),
            "argv 路径不经 url 解析，单冒号形态自然可认"
        );
        assert!(
            prefix_found(&bad),
            "argv 路径下双斜杠形态也能认 —— 所以这个 bug 在 Windows / Linux 上不可见"
        );
    }
}
