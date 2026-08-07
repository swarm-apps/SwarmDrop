# 三端日志：给用户一份能交出来的现场

> **状态：🟢 移动端已落地并实测（2026-08-07）· 🟡 桌面端已实现，待打包验证。**
>
> 触发点是补 issue 模板时发现：bug 报告里根本没法要求用户附日志。结论
> （`tracing-appender` 做文件层三端共用、平台侧各挂各的 layer）已按 openspec 的
> `mobile-logging` / `desktop-logging` 两个 change 实施。
>
> **实测结果（iOS 模拟器 + Android Pixel_7）**：日志双端落盘、iOS os_log 与
> Android logcat 均有输出、级别映射正确。唯一未自动验证的是「导出/打开」那一步的
> UI 点击（Expo 坐标点击不生效，iOS 侧要 WebDriverAgent）。
>
> **落地时推翻的两处本文原判**，已在正文对应位置更正：Android 侧现成 crate 全部停更、
> 改为自实现；`EnvFilter` 的前缀匹配没有问题（曾怀疑是空日志的原因，实测不是）。

## 缺口在哪

跨网络 P2P 的故障几乎都依赖具体网络环境（NAT 类型、运营商、是否走中继），
**在我们自己的机器上根本复现不出来**。日志是唯一的现场——而现在三端里有两端交不出来：

| 端 | tracing subscriber | 日志去了哪 | 用户能否取得 |
|---|---|---|---|
| 桌面 | `fmt::layer()` | stdout | ❌ 除非从终端启动应用 |
| **移动** | **没有** | **哪都没去** | ❌ 完全没有 |
| 浏览器 | — | 设置页的事件日志面板 | ✅ 页面上就能看 |

移动端那一行是本次调研最意外的发现，展开在下面。

## 桌面端

`src-tauri/src/setup.rs` 的 `init_tracing()` 只挂了一层 `fmt::layer()`，日志只去 stdout，
而打包后的应用是双击启动的，stdout 没有去处。

### 两条路线

**方案 A：`tauri-plugin-log`（官方插件）**

| 项 | 情况 |
|---|---|
| 日志目录 | macOS `~/Library/Logs/{bundleId}`、Windows `%LOCALAPPDATA%/{bundleId}/logs`、Linux `$XDG_DATA_HOME/{bundleId}/logs` |
| Target | `Stdout` / `LogDir` / `Webview` |
| 轮转 | 达到 `max_file_size` 后默认丢弃，或 `RotationStrategy::KeepAll` |
| 前端 | 有 JS API |
| **致命项** | **只支持 `log` crate，文档全篇不提 `tracing`** |

最后一行是决定性的。本仓从 `crates/*` 到 `src-tauri` 全用 `tracing`（还用了 span），
要让这些日志进插件得反向桥接 `tracing → log`——方向本就别扭（生态里成熟的是反过来的
`LogTracer`），还会把 span 上下文压平成一行文本。为一个文件 writer 引入第二套日志生态，
不划算。

**方案 B：`tracing-appender`（推荐）**

`RollingFileAppender::builder()` 提供 `rotation()`（`MINUTELY`/`HOURLY`/`DAILY`/`NEVER`）、
`max_log_files(n)`（只保留最近 n 个，自动删旧）、`filename_prefix()`/`filename_suffix()`、
`latest_symlink()`。接进现有代码就是多挂一层 layer，`EnvFilter` 与 span 语义原样保留。

唯一的真实限制：**只能按时间轮转，不能按大小**。缓解见「共用的实现要点」第 2 条。

「打开日志文件夹」用仓库已有的 `tauri-plugin-opener`（`src-tauri/Cargo.toml:22`），零新依赖。

## 移动端 —— 缺口比桌面大

### 现状：日志根本没有产生

`mobile/packages/swarmdrop-core/rust/mobile-core/` 里 **`tracing_subscriber` 零命中**
（依赖树里那两个命中是传递依赖，不是主动使用）。Cargo.toml 只有 `tracing`——那是**发**
日志的门面。

**没有 subscriber，所有 `tracing::info!` / `debug!` 都是空操作。** 桌面端至少还能从终端
看到，移动端是连产生都没产生。这也意味着：现在给移动端排查问题，除了复现和猜，没有别的手段。

### Android 还有一层额外的坑

即便加上 subscriber，**在 Android 上写 stdout/stderr 等于扔进 /dev/null**。
`log.redirect-stdio` 这个老办法只在 **Dalvik（Android 4.4 及更早）** 有效，
**ART（5.0+）不支持**——也就是在所有还活着的 Android 版本上都不管用。
唯一的途径是走 NDK 的 `android/log.h`，即 `tracing-logcat` / `android_logger` 这类 crate。

顺带修正一处注释：`mobile-core/src/panic_hook.rs:45` 写着
「写日志(Android logcat / iOS oslog 都会捕获 stderr)」，随后第 46 行用 `eprintln!` 输出。
**这句对 iOS 成立，对 Android 不成立。** 好在第 48–50 行同时把 panic 文本存进了
`store()`，并有 `take_last()` 可供上层读取——所以 panic 信息本身有兜底、没有丢，
只是那条注释会误导后来人以为 `eprintln!` 那行在 Android 上有用。

### 方案

Rust 侧 `tracing_subscriber::registry()` 挂三层：

| 层 | Android | iOS |
|---|---|---|
| 平台原生 | ~~`tracing-logcat`~~ → **自实现 `MakeWriter`**（见下） | [`tracing-oslog`](https://crates.io/crates/tracing-oslog)（输出到 os_log，Console.app 可见） |
| 文件 | `tracing-appender`，落在 app sandbox | 同左 |
| 过滤 | `EnvFilter`，与桌面同一套默认值 | 同左 |

> **⚠️ 2026-08-07 实施时推翻**：Android 侧两个候选 crate 都已停更
> （`tracing-logcat` 168K 下载 / 2024-07，`tracing-android` 675K / 2022-01），
> 改为**自实现约 50 行的 `MakeWriter`** 直调 NDK 的 `__android_log_write`
> ——这层的全部工作只是把字节交给 liblog，不值得为它背一个停更依赖。
> 纯逻辑与 FFI 分离，级别映射、NUL 替换、UTF-8 边界截断都能在开发机上跑测试。
> 实现见 `mobile-core/src/logging/android.rs`，决策见 openspec `mobile-logging` design D2。
> 已在 Pixel_7 实测：`adb logcat -s SwarmDrop` 有输出且优先级为 `I`（INFO），映射正确。

平台原生那层给的是**开发者**用的（`adb logcat` / Xcode Console），文件层给的是**用户**用的。
移动端用户无法从终端启动应用，所以**文件层 + 应用内导出是唯一途径**——这条比桌面端刚需得多。

uniffi 侧需要暴露两个东西：一个 `init_logging()` 供 App 启动时调用（Mozilla
application-services 也是这个形状，他们叫 `RustLog.enable()`），一个 `log_file_path()`
让 RN 拿到文件位置。

RN 侧的导出入口**依赖已经齐了**：`expo-file-system`（`mobile/package.json:63`）读文件、
`expo-sharing`（`:73`）拉起系统分享面板。设置页加一个「导出日志」即可，不需要新依赖。

### 先例

Mozilla application-services 同样是 uniffi + 移动端的组合，做法是**用各平台的原生日志设施**
而非统一回调：Android 走 logcat 且提供显式的 `RustLog.enable()` 开关，iOS 走 Xcode 调试器
并支持把日志文件拷进 container 导出。与上面的方案同构。

## 浏览器端

设置页已有事件日志面板，是三端里唯一「用户自己就能看到」的。不在本次范围。

## 共用的实现要点

1. **桌面的 `init_tracing()` 拿不到日志目录。** 它在 `tauri::Builder` 之前调用，而
   `app.path().app_log_dir()` 需要 App 实例。推荐用 `tracing_subscriber::reload`：
   先起 stdout，setup hook 里再把文件层装上。**别直接挪进 setup hook**——那会丢掉启动
   早期的日志，而那恰好是 keychain 读取、节点 bind 这些最容易出问题的阶段。

2. **文件层的级别要独立于控制台层。** 控制台保持 `debug` 方便开发，文件层收到 `info`。
   `swarmdrop_net=debug` 在 P2P 场景下事件很密，真正的控量手段是**少写**，不是勤轮转——
   这也正好绕开 `tracing-appender` 不能按大小轮转那条限制。配 `DAILY` + `max_log_files(7)` 兜底。

3. **`non_blocking()` 的 guard 必须保活。** `tracing_appender::non_blocking()` 返回
   `(writer, guard)`，guard 一 drop，后台写线程就停，**日志静默消失且不报错**。
   它得存进 App state（桌面）或核心对象（移动端）。这是这个库最经典的坑。

4. **隐私。** 日志里有 `NodeId` 和 multiaddr（含内网/公网 IP）。给出日志入口时要顺带说明，
   与 issue 模板里那句「贴之前先看一眼」对齐。移动端的「导出日志」尤其要提示——分享面板
   一按就发出去了。

## 代价估计

| | 桌面 | 移动 |
|---|---|---|
| 新依赖 | `tracing-appender` | `tracing-appender` + `tracing-logcat` + `tracing-oslog` + `tracing-subscriber` |
| 改动 | `init_tracing()` 拆两层 + 一个命令 + 设置页一个按钮 | 新建 `logging.rs` + 两个 uniffi 导出 + RN 设置页一个按钮 |
| 风险 | 低，纯增量 | 低，纯增量；但移动端要跑一次 `build:ios` 重建桥接 |

## 建议的顺序

**先移动端。** 它现在是零（连日志都不产生），而且用户没有任何替代手段；桌面端至少还有
「从终端启动」这条路给能折腾的用户。两边的文件层实现可以共用同一套写法。

## 参考

- [Tauri v2 Logging 插件文档](https://v2.tauri.app/plugin/logging/)
- [`tracing_appender::rolling::Builder`](https://docs.rs/tracing-appender/latest/tracing_appender/rolling/struct.Builder.html)
- [`tracing-logcat`](https://docs.rs/tracing-logcat) · [`tracing-oslog`](https://crates.io/crates/tracing-oslog)
- [Mozilla application-services — Logging](https://mozilla.github.io/application-services/book/logging.html)
- [Android NDK #671 — stdout/stderr 不进 logcat](https://github.com/android/ndk/issues/671)
