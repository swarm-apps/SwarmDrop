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

/// 未设 `RUST_LOG` 时控制台层的默认过滤。
///
/// `swarmdrop=debug` 靠 `EnvFilter` 的 target **前缀匹配**覆盖全部 `swarmdrop_*` crate
/// （`swarmdrop_transfer` / `swarmdrop_core` / `swarmdrop_web` …），所以传输探针
/// （[`swarmdrop_transfer::probe`]）天然放行，不必单列。
///
/// **`webrtc_p2p=info` 必须单列**：它不以 `swarmdrop` 开头，前缀匹配够不着，于是
/// webrtc-direct 数据面的告警（尤其 `udp_mux` 的支路丢包）在生产日志里**一条都不会出现**。
/// 2026-08-10 诊断「浏览器→桌面恒定 3.3 MB/s」时，头号嫌疑正是那条被日志级别屏蔽掉的
/// 丢包路径——查不到不是因为没丢，是因为看不见。取 `info` 而非 `debug`：只要告警与
/// 连接生命周期，不要每包的 trace。
const DEFAULT_FILTER: &str = "swarmdrop=debug,swarmdrop_net=debug,webrtc_p2p=info";

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
///
/// ⚠️ **这里面不能带 per-layer filter（`.with_filter()`）**。`Filtered` layer 的
/// `FilterId` 由 registry 在 `.with()` 注册那一刻分配，而经 reload 后期塞进来的拿不到，
/// 一使用就 panic：
///
/// ```text
/// a `Filtered` layer was used, but it had no `FilterId`; was it registered with the subscriber?
/// ```
///
/// 更糟的是它发生在 `did_finish_launching` 这个 ObjC 回调里，会变成 non-unwinding panic
/// 直接 abort——应用一启动就崩。所以文件层的级别过滤挂在**空位外层**（见 [`init`]），
/// 注册时就分配好 FilterId；装进来的内层是不带 filter 的裸 layer。
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
        // 文件层的级别过滤挂在**空位外层**：`Filtered` 的 FilterId 必须在注册时分配，
        // 放进 reload 内层会在装载后 panic（详见 `BoxedLayer` 的注释）。
        .with(slot.with_filter(FILE_LEVEL))
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

    // 不带 filter —— 级别过滤在 `init()` 里挂在空位外层了。这里再加一层 `.with_filter()`
    // 会因为拿不到 FilterId 而在首次写日志时 panic。
    let layer = fmt::layer()
        .with_writer(writer)
        // 文件不是终端，ANSI 转义只会变成乱码。
        .with_ansi(false)
        .with_target(true);

    handle.modify(|slot| *slot = Some(Box::new(layer))).ok()?;

    // 立刻写一行。没有它，用户在「刚启动就打开日志文件夹」时会看到一个 0 字节文件，
    // 看起来像功能坏了。这一行同时标明日志何时开始、落在哪，排查时是有用的锚点。
    tracing::info!(dir = %dir.display(), "文件日志已装载");

    Some(guard)
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
        let layer = fmt::layer().with_writer(std::io::sink).with_ansi(false);
        let _boxed: BoxedLayer = Box::new(layer);
    }

    /// 走完整的 reload 流程并**真的写一条日志**。
    ///
    /// 这条是 2026-08-07 补的，起因是原先只有上面那个「类型能装下」的编译期测试，
    /// 它给了虚假的安全感：装进去的 layer 带了 `.with_filter()`，类型完全合法、编译通过、
    /// 测试全绿，但一到运行时就
    ///
    /// ```text
    /// a `Filtered` layer was used, but it had no `FilterId`
    /// ```
    ///
    /// 而且发生在 `did_finish_launching` 的 ObjC 回调里 → non-unwinding panic → 应用
    /// 一启动就 abort。**只测类型是不够的，必须让日志真的流过一遍。**
    #[test]
    fn reload_slot_actually_logs_after_installation() {
        use std::sync::{Arc, Mutex};
        use tracing_subscriber::fmt::MakeWriter;

        #[derive(Clone)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> MakeWriter<'a> for Buf {
            type Writer = Self;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let sink = Buf(Arc::new(Mutex::new(Vec::new())));
        let (slot, handle) = reload::Layer::new(None::<BoxedLayer>);

        // 与 `init()` 同构：filter 挂在空位**外层**。
        let subscriber = tracing_subscriber::registry().with(slot.with_filter(FILE_LEVEL));

        tracing::subscriber::with_default(subscriber, || {
            // 装载前不该有输出。
            tracing::info!("before");
            assert!(sink.0.lock().unwrap().is_empty(), "装载前不应产生输出");

            // 与 `install_file_layer()` 同构：装进去的是**不带 filter** 的裸 layer。
            let layer = fmt::layer().with_writer(sink.clone()).with_ansi(false);
            handle
                .modify(|s| *s = Some(Box::new(layer)))
                .expect("装载文件层失败");

            // 关键：这一行若 panic，就是 FilterId 那个坑复发了。
            tracing::info!("after");
        });

        let out = String::from_utf8_lossy(&sink.0.lock().unwrap()).into_owned();
        assert!(
            out.contains("after"),
            "装载后应当能写入日志，实际输出: {out:?}"
        );
        assert!(!out.contains("before"), "装载前的日志不该出现");
    }

    /// 文件层的级别必须真的比控制台保守，否则 D3 的控量意图落空。
    #[test]
    fn file_level_filters_out_debug() {
        assert!(
            FILE_LEVEL < LevelFilter::DEBUG,
            "FILE_LEVEL 应比 DEBUG 保守，否则 swarmdrop_net 的高频事件会撑爆用户磁盘"
        );
    }

    /// 目录不可写时返回 None 而不是 panic —— 日志不该让应用起不来。
    #[test]
    fn install_returns_none_on_unwritable_dir() {
        // RELOAD 未设置时同样走 None 分支；这里两种失败路径都不该 panic。
        let result = install_file_layer(Path::new("/proc/nonexistent-swarmdrop-log"));
        assert!(result.is_none());
    }
}
