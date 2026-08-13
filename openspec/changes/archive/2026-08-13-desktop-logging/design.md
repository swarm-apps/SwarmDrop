## Context

`src-tauri/src/setup.rs` 的 `init_tracing()` 目前是：

```rust
tracing_subscriber::registry()
    .with(fmt::layer())                     // 只有 stdout
    .with(EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("swarmdrop=debug,swarmdrop_net=debug")))
    .init();
```

打包后的应用双击启动，stdout 没有去处，日志等于不存在。issue 模板里那段「从终端启动
应用」是在为这个缺口打补丁，且要求用户重新复现一次问题。

**与 `mobile-logging` 的关系**：移动端已经建成同一套能力，其文件层写法
（`RollingFileAppender::builder()` + `non_blocking()` + guard 保活）可直接复用，
本设计只记录桌面端**不同**的部分。移动端那边最大的难点是「日志根本没产生」，
桌面端的难点则完全不同——是**初始化时机**。

约束：

- 桌面端**已有**控制台层且开发者依赖它，重构不得改变 `pnpm tauri dev` 的现有输出。
- `tracing-appender` 已由 `mobile-logging` 加进 workspace dependencies，这里直接引用。
- 该依赖只落 `src-tauri`，不进共享 crate，`check-wasm.sh` 必须保持绿。

## Goals / Non-Goals

**Goals:**

- 打包后的桌面应用留下可取回的日志现场。
- 保留启动早期（keychain 读取、节点 bind）的日志。
- 用户能自己找到日志文件。
- 开发期控制台行为零变化。

**Non-Goals:**

- 浏览器端（设置页已有事件日志面板）。
- 远程日志上报 / 崩溃收集。本项目无遥测。
- 应用内日志查看器。到「能打开文件夹」为止。

## Decisions

### D1：文件层复用 `mobile-logging` 的写法

`RollingFileAppender::builder()` 配 `rotation(DAILY)` + `max_log_files(n)`，
外面套 `non_blocking()`。参数与移动端保持一致，避免两端行为分叉。

- 该库**只能按时间轮转、不能按大小**。控量靠 D3 的级别分层，不靠勤轮转。

### D2：用 `reload` layer 解决初始化时机（本次的核心难点）

`init_tracing()` 在 `tauri::Builder` 之前调用，此时 `app.path().app_log_dir()` 拿不到
——它需要 App 实例。

- **备选 A**：把整个 `init_tracing()` 挪进 setup hook。**否决** —— 会丢掉启动早期的日志，
  而 keychain 读取与节点 bind 恰好在那之前，且它们正是最容易出问题的阶段。
- **备选 B**：在 Rust 侧自行按平台推导日志目录，不走 `PathResolver`。**否决** ——
  会把目录约定复制一份，将来与 Tauri 的 `app_log_dir()` 悄悄漂移。
- **采用**：`tracing_subscriber::reload`。先以「文件层为空」注册订阅器，
  setup hook 里拿到目录后再把真正的文件层装进去。

实施要点：reload 的类型参数需要在注册时就固定下来，因此空位应使用
`Option<Box<dyn Layer<..> + Send + Sync>>` 这类可容纳后续实现的类型，
而不是具体的 `fmt::Layer<..>`——后者会把 writer 类型烤进签名，装载时对不上。

代价：启动早期到 setup hook 之间的日志只进控制台、不进文件。这段窗口很短，
且那段日志开发者本来就在终端看得到，可接受。

### D3：控制台与文件用不同级别

控制台保持现有默认（`swarmdrop=debug,swarmdrop_net=debug`），文件层收更保守的级别。
`swarmdrop_net` 事件密集，两层同级会让用户磁盘快速增长。这也是 D1 那条「不能按大小
轮转」限制的正解——控量手段是**少写**。

### D4：给「打开日志文件夹」，不给「导出日志」

- **备选**：像移动端那样做导出 + 分享。**否决** —— 桌面端有文件管理器，打开目录是更自然
  的交互；而且用户能先自己看一眼再决定发不发，这对含设备 ID 与 IP 的日志是更好的默认。
- 用仓库已有的 `tauri-plugin-opener`（`src-tauri/Cargo.toml:22`），零新依赖。
- 移动端之所以做导出，是因为那里没有文件管理器这个概念——两端交互不同是场景决定的，
  不是不一致。

### D5：`non_blocking` 的守卫存进 Tauri state

与移动端同一条：guard 一旦 drop，后台写线程停止，**日志静默消失且不报错**。
桌面端存进 Tauri 的 managed state（而非移动端那种进程级 `OnceLock`），随 App 生命周期。
需要一条测试兜底。

## Risks / Trade-offs

- **[守卫被释放导致日志静默中断]** → 存进 managed state；补测试。与移动端同一风险，
  同一缓解。
- **[reload 的类型参数选错，装载时对不上]** → 空位用 `Option<Box<dyn Layer>>`；
  这条在实施中会立刻由编译器暴露，不是运行期风险。
- **[启动早期到 setup hook 之间的日志不落盘]** → 接受。窗口很短，且那段在终端可见。
- **[日志泄露设备标识与网络地址]** → 入口处提示；口径与 issue 模板一致。
  桌面端「先打开文件夹再由用户决定」本身就比一键分享更保守。
- **[开发期控制台行为被意外改变]** → 重构后需人工确认 `pnpm tauri dev` 的输出与之前一致。

## Migration Plan

纯增量，无数据迁移、无破坏性变更。

- 新增 IPC 命令，`bindings.ts` 由 `pnpm tauri dev` 自动重新导出，不得手改。
- 回滚即移除文件层与设置页入口，`init_tracing()` 退回单层，不留残余状态。

## Open Questions

- 文件层级别与保留天数是否与移动端取同一组值？倾向一致（便于两端对照日志），
  但桌面磁盘更宽裕，可能允许更长保留期。
- 是否顺带把 `src-tauri/Cargo.toml` 里自带版本号的 `tracing-subscriber` 收敛到
  workspace dependencies？属于清理，与本能力无关，可单独做。
