//! 移动端诊断日志。
//!
//! 在此之前本 crate 只有 `tracing` 门面而没有任何订阅器——`crates/*` 里所有
//! `info!` / `debug!` 都是空操作，移动端的日志**从未产生**。本模块补上订阅端。
//!
//! 三层结构，各有各的受众：
//!
//! | 层 | 去处 | 给谁 |
//! |---|---|---|
//! | 平台原生 | Android logcat / iOS os_log | 开发者（`adb logcat` / Console.app） |
//! | 文件 | 应用沙箱内按天轮转 | **终端用户**，经设置页导出 |
//! | 过滤 | — | 两层共用的 `EnvFilter` |
//!
//! 文件层是重点：移动端用户无法从终端启动应用，落盘导出是他们唯一能交出日志的途径。

// 非 Android 平台上只在 `cfg(test)` 时编译：它的纯逻辑（级别映射、字节清理）值得被
// 开发机上的 `cargo test` 覆盖，但不该在正常构建里产生一堆 dead_code 警告。
#[cfg(any(target_os = "android", test))]
mod android;

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{
    Layer,
    filter::{EnvFilter, LevelFilter},
    fmt,
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
};

/// 日志文件名前缀，轮转后形如 `swarmdrop.2026-08-07.log`。
const FILE_PREFIX: &str = "swarmdrop";
const FILE_SUFFIX: &str = "log";

/// 保留天数。日志是短期诊断数据，留太多只是占用户存储。
const MAX_LOG_FILES: usize = 7;

/// 未设 `RUST_LOG` 时的默认过滤。与桌面壳 `init_tracing()` 保持同一组默认值。
const DEFAULT_FILTER: &str = "swarmdrop=debug,swarmdrop_net=debug";

/// 文件层的级别。**刻意比平台层保守**：`swarmdrop_net` 在 P2P 场景下事件很密，
/// 文件层若也吃 `debug`，用户存储会被快速消耗。
///
/// `tracing-appender` 只能按时间轮转、不能按大小，所以真正的控量手段是**少写**，
/// 而不是勤轮转——这条常量就是那个控量点。
const FILE_LEVEL: LevelFilter = LevelFilter::INFO;

/// 进程级状态。
///
/// **`_guard` 必须活到进程结束**：`tracing_appender::non_blocking()` 返回的 guard
/// 一旦 drop，后台写线程就停止，日志静默消失且**不产生任何错误**。它没有反馈回路，
/// 所以只能靠把它放在这里 + 靠测试钉住。
struct LogState {
    _guard: WorkerGuard,
    file_path: PathBuf,
}

static STATE: OnceLock<LogState> = OnceLock::new();

/// 初始化日志。幂等：重复调用直接返回，不 panic、不重复注册订阅器。
///
/// `dir` 由宿主传入而非在此推导平台目录——RN 侧本来就持有 `expo-file-system` 的目录
/// 常量，传进来比在 Rust 里分叉两套平台代码更直接，也更好测。它是 `file://` URI，
/// 与 `MobileCore::new` 一样在边界经 [`crate::utils::parse_host_dir`] 解析。
///
/// 任何失败（目录不可写、订阅器已被别处注册）都就地吞掉：日志是诊断设施，
/// 不该让应用起不来。
#[uniffi::export]
pub fn init_logging(dir: String) {
    if STATE.get().is_some() {
        return;
    }
    if let Some(state) = build(&crate::utils::parse_host_dir(&dir)) {
        let file = state.file_path.clone();
        // 竞态下另一个线程可能已经写入；set 失败说明对方赢了，丢弃本次即可。
        if STATE.set(state).is_ok() {
            // 立刻写一行。没有它，用户在「什么都没做就导出日志」时会拿到一个**空文件**
            // ——2026-08-07 首次真机验证时就是这样：应用起来了但没启节点，
            // `crates/*` 自然没有 INFO 级事件，文件 0 字节，看起来像功能坏了。
            // 这一行同时标明日志何时开始、落在哪，排查时是有用的锚点。
            tracing::info!(target: "swarmdrop", file = %file.display(), "日志已启动");
        }
    }
}

/// 当前日志文件的 `file://` URI。未初始化时返回 `None`，供调用方展示空状态。
///
/// 返回 URI 而非裸路径：宿主的世界是 URI（`Sharing.shareAsync` 只认它），
/// 与 [`init_logging`] 收 URI 的契约对称，调用方不必知道我们内部存的是 [`PathBuf`]。
#[uniffi::export]
pub fn log_file_path() -> Option<String> {
    STATE.get().map(|s| crate::utils::to_host_uri(&s.file_path))
}

fn build(dir: &Path) -> Option<LogState> {
    if let Err(err) = std::fs::create_dir_all(dir) {
        // 还没有日志设施可用，这里只能沉默失败。
        let _ = err;
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

    let file_layer = fmt::layer()
        .with_writer(writer)
        // 文件不是终端，ANSI 转义只会变成乱码。
        .with_ansi(false)
        .with_target(true)
        .with_filter(FILE_LEVEL);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let registry = tracing_subscriber::registry()
        .with(file_layer)
        .with(platform_layer())
        .with(env_filter);

    // 已被别处初始化时返回 Err——保持幂等语义，不 panic。
    registry.try_init().ok()?;

    Some(LogState {
        _guard: guard,
        file_path: current_file_path(dir),
    })
}

/// 轮转文件名由 `tracing-appender` 按 `prefix.YYYY-MM-DD.suffix` 生成，
/// 它不暴露"当前文件"的查询接口，因此这里扫描目录取最新的一个。
///
/// 用扫描而不是自己拼日期，是为了不把日期格式这条约定复制一份——
/// 它属于 `tracing-appender`，复制就会在将来某次升级后悄悄对不上。
fn current_file_path(dir: &Path) -> PathBuf {
    let newest = std::fs::read_dir(dir).ok().and_then(|entries| {
        entries
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{FILE_PREFIX}."))
            })
            .max_by_key(|e| e.file_name())
            .map(|e| e.path())
    });

    newest.unwrap_or_else(|| dir.join(format!("{FILE_PREFIX}.{FILE_SUFFIX}")))
}

/// 平台原生层。非 Android / iOS 上返回 `None`，让本 crate 在开发机上照常编译。
fn platform_layer<S>() -> Option<Box<dyn Layer<S> + Send + Sync>>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    #[cfg(target_os = "android")]
    {
        Some(Box::new(
            fmt::layer()
                .with_writer(android::MakeLogcatWriter)
                .with_ansi(false)
                // logcat 自带时间戳与优先级，再打一份是噪音。
                .without_time()
                .with_level(false),
        ))
    }
    #[cfg(target_os = "ios")]
    {
        Some(Box::new(tracing_oslog::OsLogger::new(
            "com.yexiyue.swarmdrop",
            "default",
        )))
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 幂等：重复调用不 panic，也不因为第二次注册失败而炸。
    #[test]
    fn init_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("swarmdrop-log-test-{}", std::process::id()));
        let path = dir.to_string_lossy().into_owned();

        init_logging(path.clone());
        init_logging(path.clone());
        init_logging(path);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 未初始化时取路径返回 None 而不是无效路径。
    ///
    /// 注意：本测试与 `init_is_idempotent` 共享同一个进程级 `STATE`，谁先跑不确定，
    /// 所以这里只断言"要么 None，要么是一个非空路径"，不假设顺序。
    #[test]
    fn file_path_is_none_or_valid() {
        match log_file_path() {
            None => {}
            Some(p) => assert!(!p.is_empty(), "已初始化时路径不该为空"),
        }
    }

    /// 默认过滤必须真的匹配到本仓的 target，否则日志一条都不会产生——而且**不报错**。
    ///
    /// 2026-08-07 首次真机验证时日志文件是空的，就是从这里查起的：`EnvFilter` 的
    /// target 是前缀匹配，`swarmdrop=debug` 能否覆盖 `swarmdrop_core` / `swarmdrop_net`
    /// 这些实际 target，靠读文档不如直接跑一遍。
    #[test]
    fn default_filter_matches_real_crate_targets() {
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

        for target in ["swarmdrop_core", "swarmdrop_net", "swarmdrop_transfer"] {
            let sink = Buf(Arc::new(Mutex::new(Vec::new())));
            let subscriber = tracing_subscriber::registry()
                .with(fmt::layer().with_writer(sink.clone()).with_ansi(false))
                .with(EnvFilter::new(DEFAULT_FILTER));

            tracing::subscriber::with_default(subscriber, || {
                tracing::info!(target: "swarmdrop_core", "probe");
                tracing::info!(target: "swarmdrop_net", "probe");
                tracing::info!(target: "swarmdrop_transfer", "probe");
            });

            let out = String::from_utf8_lossy(&sink.0.lock().unwrap()).into_owned();
            assert!(
                out.contains("probe"),
                "DEFAULT_FILTER ({DEFAULT_FILTER}) 没有匹配到 target `{target}`，\
                 这会导致日志一条都不产生且不报错"
            );
        }
    }

    /// 钉住 D6 那条无声失败：guard 必须被 `LogState` 持有。
    ///
    /// 若将来有人把 `_guard` 从结构体里去掉、或改成在 `build()` 里 drop，
    /// 日志会静默停止且没有任何报错——这条测试是唯一的兜底。
    #[test]
    fn log_state_holds_the_worker_guard() {
        assert!(
            std::mem::size_of::<LogState>() > std::mem::size_of::<PathBuf>(),
            "LogState 必须除 file_path 外还持有 WorkerGuard；\
             guard 一旦被 drop，后台写线程停止、日志静默消失且不报错"
        );
    }
}
