## Why

桌面端的日志只去 stdout：`src-tauri/src/setup.rs` 的 `init_tracing()` 只挂了一层
`fmt::layer()`。而打包后的应用是双击启动的，**stdout 没有去处**——用户报 bug 时交不出
任何现场。

这对跨网络 P2P 尤其贵：故障几乎都依赖具体网络环境（NAT 类型、运营商、是否走中继），
开发者本地根本复现不出来，日志是唯一线索。

现状是拿补丁顶着：仓库 issue 模板的日志字段里写了三个平台「从终端启动应用」的命令。
那条路只对愿意折腾的用户有效，而且要求他们**重新复现一次问题**。

`mobile-logging` 已经为移动端补上了同一套能力，其文件层的写法可直接复用。

## What Changes

- **`init_tracing()` 拆成两层**：控制台层保持现状（开发期不受影响），新增文件层。
- **文件层用 `tracing-appender`** 写入 Tauri 约定的应用日志目录，按天轮转、限制保留数量。
  级别比控制台层保守，以控制写入量。
- **解决初始化时机问题**：`init_tracing()` 在 `tauri::Builder` 之前调用，此时拿不到
  `app.path().app_log_dir()`。改用 `tracing_subscriber::reload`，先起控制台层，
  在 setup hook 里再把文件层装上。
  - 不把整个初始化挪进 setup hook：那会丢掉启动早期的日志，而 keychain 读取、节点 bind
    这些最容易出问题的阶段恰好在那之前。
- **设置页新增「打开日志文件夹」**，走仓库已有的 `tauri-plugin-opener`。
  比「导出日志」更简单，且用户能先自己看一眼再决定发不发。
- **入口处提示隐私**：日志含设备标识与网络地址，口径与 issue 模板一致。

非目标：浏览器端（设置页已有事件日志面板）；远程日志上报（本项目无遥测）；
日志内 UI 查看器。

## Capabilities

### New Capabilities

- `desktop-diagnostic-log`: 桌面端诊断日志的落盘、轮转、级别分层，以及用户可发起的
  「打开日志文件夹」。

### Modified Capabilities

无。纯增量，不改变任何既有能力的需求。

## Impact

**代码**

- `src-tauri/src/setup.rs` — `init_tracing()` 重构成可 reload 的两层结构。
- `src-tauri/src/commands/` — 新增一个打开日志目录的命令薄壳。
- `src-tauri/src/setup.rs` 的 `collect_commands!` — 注册新命令。
- `src/routes/_app/settings/` — 设置页新增入口。

**依赖**

- 新增 Rust crate 1 个：`tracing-appender`（`mobile-logging` 已把它加进
  workspace dependencies，这里直接引用即可）。
- 零新 JS 依赖。

**构建与门禁**

- 改了 IPC 命令面，`bindings.ts` 会由 `pnpm tauri dev` 自动重新导出，**不得手改**。
- `tracing-appender` 只落 `src-tauri`，不进任何共享 crate，
  `./scripts/check-wasm.sh` 必须保持绿。

**隐私**

- 日志含 `NodeId` 与 multiaddr（内网/公网 IP）。入口处必须给出提示。

**后续**

- 落地后可把 issue 模板 `bug_report.yml` 的日志字段从「桌面端没有日志文件，需从终端
  启动」改为「设置页打开日志文件夹」。
