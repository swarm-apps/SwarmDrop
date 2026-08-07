//! 桌面端诊断日志。
//!
//! 此前 `init_tracing()` 只挂了一层 `fmt::layer()`，日志只去 stdout——而打包后的应用
//! 是双击启动的，stdout 没有去处，用户报 bug 时交不出任何现场。本模块补上文件层。
//!
//! ## 为什么要 reload
//!
//! 日志订阅必须在 `tauri::Builder` **之前**注册，否则 keychain 读取、节点 bind 这些
//! 启动早期（且最容易出问题）的日志会丢。可那时候 `app.path().app_log_dir()` 还拿不到
//! ——它需要 App 实例。
//!
//! 于是分两步：[`init`] 先注册订阅器、文件层留一个空位；等 setup hook 里拿到目录，
//! 再由 [`install_file_layer`] 把真正的文件层装进去。代价是这两步之间的日志只进控制台，
//! 窗口很短且那段开发者本来就在终端看得到。
//!
//! 文件层的写法与移动端 (`mobile-core` 的 `logging` 模块) 保持一致，参数也对齐，
//! 免得两端日志行为分叉。

use std::{path::Path, sync::OnceLock};

use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{
    Layer, Registry,
    filter::{EnvFilter, LevelFilter},
    fmt,
    layer::SubscriberExt,
    reload,
    util::SubscriberInitExt,
};

/// 日志文件名前缀，轮转后形如 `swarmdrop.2026-08-07.log`。
const FILE_PREFIX: &str = "swarmdrop";
const FILE_SUFFIX: &str = "log";

/// 保留天数。与移动端取同一个值，便于两端对照日志。
const MAX_LOG_FILES: usize = 7;

/// 未设 `RUST_LOG` 时控制台层的默认过滤。**与重构前完全一致**——开发期行为不受本模块影响。
const DEFAULT_FILTER: &str = "swarmdrop=debug,swarmdrop_net=debug";

/// 文件层的级别，**刻意比控制台保守**。
///
/// `swarmdrop_net` 在 P2P 场景下事件密集，两层同级会让用户磁盘快速增长。
/// `tracing-appender` 只能按时间轮转、不能按大小，所以真正的控量手段是**少写**，
/// 这个常量就是那个控量点。
const FILE_LEVEL: LevelFilter = LevelFilter::INFO;

/// 文件层的空位类型。
///
/// **必须是 `Box<dyn Layer<..>>` 而不是具体的 `fmt::Layer<..>`**：后者会把 writer 类型
/// 烤进签名，等到 [`install_file_layer`] 真正装载时就对不上了。
type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync>;
type ReloadHandle = reload::Handle<Option<BoxedLayer>, Registry>;

static RELOAD: OnceLock<ReloadHandle> = OnceLock::new();

/// 注册订阅器：控制台层立即生效，文件层留空位。
///
/// 在 `tauri::Builder` 之前调用。重复调用是无害的（`try_init` 失败即返回）。
pub fn init() {
    let (slot, handle) = reload::Layer::new(None::<BoxedLayer>);

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let registered = tracing_subscriber::registry()
        .with(slot)
        .with(fmt::layer())
        .with(filter)
        .try_init()
        .is_ok();

    if registered {
        let _ = RELOAD.set(handle);
    }
}

/// 在应用日志目录上装载文件层。
///
/// 返回的 [`WorkerGuard`] **必须被调用方保活到应用退出**：它一旦 drop，
/// `non_blocking` 的后台写线程就停止，日志静默消失且**不产生任何错误**。
/// 调用点应把它存进 Tauri 的 managed state。
///
/// 任何失败（目录不可写、订阅器未注册）都返回 `None`，不阻断应用启动——
/// 日志是诊断设施，不该让应用起不来。
#[must_use = "guard 一旦被 drop，日志会静默停止；请存进 managed state"]
pub fn install_file_layer(dir: &Path) -> Option<WorkerGuard> {
    let handle = RELOAD.get()?;

    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }

    let appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix(FILE_PREFIX)
        .filename_suffix(FILE_SUFFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .ok()?;

    let (writer, guard) = tracing_appender::non_blocking(appender);

    let layer = fmt::layer()
        .with_writer(writer)
        // 文件不是终端，ANSI 转义只会变成乱码。
        .with_ansi(false)
        .with_target(true)
        .with_filter(FILE_LEVEL);

    handle
        .modify(|slot| *slot = Some(Box::new(layer)))
        .ok()
        .map(|()| guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 空位类型必须能容纳任意 writer 的文件层。
    ///
    /// 若有人把 [`BoxedLayer`] 改成具体的 `fmt::Layer<..>`，这里会编译失败——
    /// 那正是 design D2 警告的那个坑：类型烤死后装载时对不上。
    #[test]
    fn boxed_layer_slot_accepts_a_file_layer() {
        let layer = fmt::layer()
            .with_writer(std::io::sink)
            .with_ansi(false)
            .with_filter(FILE_LEVEL);
        let _boxed: BoxedLayer = Box::new(layer);
    }

    /// 目录不可写时返回 None 而不是 panic —— 日志不该让应用起不来。
    #[test]
    fn install_returns_none_on_unwritable_dir() {
        // RELOAD 未设置时同样走 None 分支；这里两种失败路径都不该 panic。
        let result = install_file_layer(Path::new("/proc/nonexistent-swarmdrop-log"));
        assert!(result.is_none());
    }
}
