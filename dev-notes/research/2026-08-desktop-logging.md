# 桌面端日志落盘：给用户一份能交出来的日志

> **状态：🟡 已调研，待决策（2026-08-07）。**
> 结论倾向**方案 B（`tracing-appender`）**，理由是选型被现有技术栈单方面决定了——
> 官方的 `tauri-plugin-log` 只吃 `log` crate，而本仓从上到下是 `tracing`。
> 触发点是补 issue 模板时发现：bug 报告里根本没法要求用户附日志。

## 缺口在哪

`src-tauri/src/setup.rs` 的 `init_tracing()` 只挂了一层 `fmt::layer()`：

```rust
tracing_subscriber::registry()
    .with(fmt::layer())                     // ← 只有 stdout
    .with(EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("swarmdrop=debug,swarmdrop_net=debug")))
    .init();
```

**日志只去 stdout，而打包后的应用是双击启动的，stdout 没有去处。** 于是：

- 用户报「传到一半断了」，我们要不到任何日志；
- 唯一的办法是让用户从终端重新启动应用复现一遍——这对非开发者用户基本等于没有；
- issue 模板里那段「怎么拿日志」写了三个平台的命令，本质上是在为这个缺口打补丁。

对一个跨网络 P2P 应用来说这个缺口尤其贵：故障往往依赖具体网络环境（NAT 类型、
运营商、是否走中继），**在我们自己的机器上根本复现不出来**，日志是唯一的现场。

## 两条路线

### 方案 A：`tauri-plugin-log`（官方插件）

| 项 | 情况 |
|---|---|
| 日志目录 | macOS `~/Library/Logs/{bundleId}`、Windows `%LOCALAPPDATA%/{bundleId}/logs`、Linux `$XDG_DATA_HOME/{bundleId}/logs` |
| Target | `Stdout` / `LogDir` / `Webview` 三选多 |
| 轮转 | 达到 `max_file_size` 后默认**丢弃**旧文件，或 `RotationStrategy::KeepAll` 保留 |
| 前端 | 有 JS API（`info()` / `error()` / `attachConsole()` …） |
| **致命项** | **只支持 `log` crate，官方文档全篇不提 `tracing`** |

最后一行是决定性的。本仓 `crates/*` 与 `src-tauri` 全部用 `tracing`（还用了 span），
要让这些日志进入插件，得反向桥接 `tracing → log`。这个方向本就别扭（生态里成熟的是
`log → tracing` 的 `LogTracer`），而且会把 span 上下文压平成一行文本，白丢结构化信息。
为了一个文件 writer 引入第二套日志生态，不划算。

### 方案 B：`tracing-appender`（tracing 生态原生）

`RollingFileAppender::builder()` 提供：

| 方法 | 作用 |
|---|---|
| `rotation()` | `MINUTELY` / `HOURLY` / `DAILY` / `NEVER` |
| `max_log_files(n)` | 只保留最近 n 个，自动删旧 |
| `filename_prefix()` / `filename_suffix()` | 文件名 |
| `latest_symlink()` | 指向当前文件的软链 |

接进现有代码就是**多挂一层 layer**，`EnvFilter` 与 span 语义原样保留。

**唯一的真实限制：只能按时间轮转，不能按大小。** 对本项目要留意——P2P 的网络事件很密，
`swarmdrop_net=debug` 跑一天的量不小。缓解办法见下面「实现要点」第 2 条。

真要按大小轮转，得自己拿 `file-rotate` 之类实现 `MakeWriter`，复杂度陡增，不值得。

## 推荐

**方案 B。** 新依赖只有 `tracing-appender` 一个，其余全是现成的：`tauri-plugin-opener`
仓库里已经有了（`src-tauri/Cargo.toml:22`），「打开日志文件夹」不需要引入任何东西。

## 实现要点与已知坑

1. **`init_tracing()` 现在拿不到日志目录。** 它在 `tauri::Builder` 之前调用，而
   `app.path().app_log_dir()` 需要 App 实例。三种解法，按推荐排序：
   - 用 `tracing_subscriber::reload`，先起 stdout，setup hook 里再把文件层装上；
   - 把整个 `init_tracing()` 挪进 setup hook（**代价：丢掉启动早期的日志**，而启动
     期恰好是 keychain 读取、节点 bind 这些最容易出问题的阶段，不推荐）；
   - 自己按平台算路径，不走 Tauri 的 PathResolver（能用，但与 `app_log_dir()` 的
     约定漂移，将来两处不一致）。

2. **两层用不同级别。** stdout 保持 `debug` 方便开发；文件层收到 `info`，否则磁盘
   增长很快。这也正好绕开「不能按大小轮转」那条限制——真正的控制手段是**少写**，
   不是勤轮转。配 `max_log_files(7)` + `DAILY` 作为兜底。

3. **`non_blocking` 的 guard 必须保活。** `tracing_appender::non_blocking()` 返回
   `(writer, guard)`，guard 一 drop，后台写线程就停，**日志静默消失、不报错**。
   它得存进 App state 或泄漏成 `'static`，这是这个库最经典的坑。

4. **隐私。** 日志里有 `NodeId` 和 multiaddr（含内网/公网 IP）。UI 上给出日志入口时
   要顺带说明这一点——issue 模板里已经写了「贴之前先看一眼」，应用内也该有同样提示。

5. **UI 入口。** 设置页「关于」或「网络」区加一个「打开日志文件夹」，走
   `tauri-plugin-opener`。比「导出日志」简单得多，而且用户能自己先看一眼再决定发不发。

## 代价估计

| 项 | 量 |
|---|---|
| 新依赖 | 1（`tracing-appender`） |
| 改动 | `init_tracing()` 重构成两层 + 一个 Tauri 命令 + 设置页一个按钮 |
| 风险 | 低。纯增量，不碰任何现有链路 |

## 移动端与浏览器端

不在本次范围，但顺带记下现状：移动端日志走 uniffi 桥接过去的 tracing，同样只到
stdout（Android 可用 `adb logcat`，iOS 可用 Xcode Console）；浏览器端在设置页已经有
事件日志面板。三端里**只有桌面端是完全拿不到的那个**，所以先解决它。

## 参考

- [Tauri v2 Logging 插件文档](https://v2.tauri.app/plugin/logging/)
- [`tracing_appender::rolling::Builder`](https://docs.rs/tracing-appender/latest/tracing_appender/rolling/struct.Builder.html)
