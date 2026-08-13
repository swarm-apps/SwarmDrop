## Why

移动端的 `tracing` 日志**根本没有产生**：`mobile-core` 里没有任何 `tracing_subscriber` 初始化，
`Cargo.toml` 只有 `tracing` 这个发日志的门面。没有 subscriber，`crates/*` 中所有 `info!` /
`debug!` 都是空操作。

后果是移动端出问题时没有任何现场可查。这对本项目尤其致命——跨网络 P2P 的故障几乎都依赖具体
网络环境（NAT 类型、运营商、是否走中继），**开发者本地根本复现不出来**，日志是唯一线索。
桌面端至少还有「从终端启动应用」这条路给能折腾的用户，移动端连这条路都没有：用户无法从终端
启动 App。三端里移动端是唯一完全取不到日志的。

## What Changes

- **Rust 侧初始化 `tracing_subscriber`**，挂三层：平台原生层、文件层、`EnvFilter` 过滤层。
- **平台原生层按 target 分叉**：Android 用 `tracing-logcat`，iOS 用 `tracing-oslog`。
  这一层服务开发者（`adb logcat` / Console.app）。
  - 必须走原生日志 API：Android 上写 stdout/stderr 等于扔进 `/dev/null`，
    老办法 `log.redirect-stdio` 只在 Dalvik（4.4 及更早）有效，ART（5.0+）不支持。
- **文件层用 `tracing-appender`** 落在 app sandbox，按天轮转并限制保留数量。
  这一层服务终端用户，是他们唯一能取到日志的途径。
- **uniffi 新增两个导出**：`init_logging()` 供 App 启动时调用，`log_file_path()` 让 RN 侧
  拿到文件位置。
- **RN 设置页新增「导出日志」入口**，走已有的 `expo-file-system` + `expo-sharing`
  拉起系统分享面板；导出前提示日志含设备 ID 与网络地址。
- **修正 `mobile-core/src/panic_hook.rs:45` 的注释**：它写着「Android logcat / iOS oslog 都会
  捕获 stderr」，该说法对 Android 不成立。panic 信息本身有 `store()` + `take_last()` 兜底
  未丢失，但注释会误导后来人以为那行 `eprintln!` 在 Android 上有效。

非目标（本次不做）：桌面端与浏览器端的日志改造。桌面端有独立缺口与独立选型结论，
见 `dev-notes/research/2026-08-logging.md`；文件层的实现可在后续复用本次的写法。

## Capabilities

### New Capabilities

- `mobile-diagnostic-log`: 移动端的诊断日志能力——日志的产生、分级、落盘轮转、
  平台原生输出，以及用户可发起的导出。

### Modified Capabilities

无。本次不改变任何既有能力的需求，是纯增量。

## Impact

**代码**

- `mobile/packages/swarmdrop-core/rust/mobile-core/` — 新增 logging 模块；`lib.rs` 增加
  uniffi 导出；`panic_hook.rs` 修注释。
- `mobile/src/` — 设置页新增导出入口。

**依赖**

- 新增 Rust crate 4 个：`tracing-subscriber`、`tracing-appender`、`tracing-logcat`（Android）、
  `tracing-oslog`（iOS）。后两者按 target 条件依赖。
- **零新 JS 依赖** —— `expo-file-system` 与 `expo-sharing` 已在 `mobile/package.json`。

**构建**

- 改了 uniffi 导出面，需重建桥接（`pnpm --filter react-native-swarmdrop-core build:ios`）
  并重新生成 TS 绑定。
- 新增依赖会进根 Cargo workspace，`cargo check --workspace` 一并覆盖；
  但这几个 crate 都是移动端专用，**不得让 `./scripts/check-wasm.sh` 变红**——
  依赖必须按 target 门控，不能无条件加进共享 crate。

**隐私**

- 日志含 `NodeId` 与 multiaddr（内网/公网 IP）。导出入口必须给出提示，与 issue 模板中
  「贴之前先看一眼」的口径一致。
