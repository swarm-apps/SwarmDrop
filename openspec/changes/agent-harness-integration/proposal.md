## Why

SwarmDrop 的瓶颈不是技术是分发：12 个 crate、自研传输栈、四端已发布，**18 ★**；而
`dsh-pocket`（往 DeepSeek Harness 塞了段反代）建仓三天 **121 ★**。文件传输是「推」的场景——
用户已经有 AirDrop 和微信，不会主动来找；而「我在跑 agent，它产出的东西我想直接拿到手上」
是「拉」的场景，dsh 的 awesome 列表专门开了 Remote & Mobile 分类、26 家在抢，需求侧是真的。

那 26 家抢的是**「人 → 机器」**（把 3080 端口暴露出去再加个 token，全都依赖公网可达或
第三方账号）。本变更明确不进那一层。要做的是**「机器 → 人」以及 agent 手边的设备通道**：
让一个跑在 dsh 里的 agent 能把产出直接投到你手机上，也能引用你从手机发过来的东西——
不需要公网 IP，不需要任何第三方账号。这件事目前无人做，而它正好是 SwarmDrop 的本行。

## What Changes

- **`crates/cli` 新增 `swarmdrop mcp`**：stdio MCP server，把发送 / 设备 / 收件箱 / 传输
  暴露成模型可调的工具。选 stdio 不选 HTTP 是因为它不占端口、进程生命周期跟随宿主、
  且无需任何 loopback 授权面。它**不只服务 dsh**——Claude Code、Codex 等 harness 接入
  SwarmDrop 的唯一公约数就是「能执行一条命令」。
- **`crates/cli` 新增机器可读的事件订阅面**：现有 `transfer watch --json` 是**轮询全量快照**
  （每 tick 打印一次未完成传输的数组），覆盖不到收件箱新增与设备上下线，消费方还得自己
  diff 才知道变了什么。新增一条推送式、事件驱动的订阅，覆盖收件箱 / 传输 / 设备三类变化。
- **收件箱领域补上一等事件**（`crates/transfer` → 四端）。调研（2026-08-21）查实：`CoreEvent`
  里没有任何收件箱域事件，于是**三端各自从 `TransferCompleted` 推导「收件箱变了」，且已经
  漂移**——移动端不按 `direction` 过滤（发送完成也白刷一次收件箱），而桌面端**根本没有**
  反应式刷新（文件到达时收件箱页零更新）。CLI 若照现状推导会成为第四份，且是最差的一份：
  它要依赖一条只以行内注释存在、零护栏测试的顺序不变量。两个条目创建点**都已经握着刚建
  出来的 `InboxItemDetail` 与事件 sink**（`receiver.rs:1038` 的返回值目前被注释标着「刻意
  不消费」），补事件不需要新造任何数据通路。
- **归档与软删收进共享编排并发事件**。`archive_inbox_item` 目前被 4 个宿主 + 2 个 MCP server
  直调、连编排层都没有，于是「桌面 MCP 归档了一条」对桌面 UI 完全不可见。收进编排后全端
  可靠，这也是订阅面能如实承诺「覆盖归档/删除」的前提。
- **SIGTERM 与 SIGINT 同等以成功退出**。全仓零 SIGTERM 处理（4 处 `ctrl_c()`，Unix 上只接
  SIGINT），而 dsh 的 `terminate()` 与 systemd stop 都先发 SIGTERM——照现状，**最常见的正常
  收摊路径永远退非零**，正好触发「消费方把正常结束读成崩溃并重启」。这是既有缺陷，与本变更
  的长驻命令直接相关，一并修。
- **事件带版本字段**：订阅面的每条事件携带 schema 版本。它是本仓的对外契约，且消费方
  （插件）会把它转写进用户的 dsh session 日志——那些日志是持久的，格式一旦漂移，旧会话
  就回放不出来。
- **独立仓 `dsh-swarmdrop`**（TS 插件，**不在本仓**）：UI 全部走 dsh 官方 seam——
  `inputTriggers` source 做 `@` 引用收件箱、`ConversationNodeDefinition` 做传输进度富渲染、
  `ctx.slots` 做设备面板与设置卡、`CommandDefinition` 做人触发的动作。
- 工具实现按**平台中立 trait 边界**设计（不碰 `tauri::AppHandle`），落在 `crates/cli` 内部
  模块，将来整体上移到共享 crate 时是搬文件而不是重写。

**不做**（各有理由，写在 design）：远程访问 dsh Web UI（26 家在抢的红海）；替换 dsh 的
`ctx.connection` carrier（依赖 developer-preview 的内部包）；「agent → 人」的审批 / 提问
转发到手机（要动移动端 UI 与授权模型，独立立项）。

## Capabilities

### New Capabilities

- `cli-mcp-host`: `swarmdrop mcp` 子命令——stdio MCP server 的协议形态、工具面、节点接入档
  与生命周期。工具集合与桌面端的 `mcp-*` 系列语义对齐，但宿主与传输完全不同（桌面端是
  Tauri 进程内的 streamable-http，见 `mcp-server`；这里是独立进程的 stdio）。
- `cli-event-stream`: CLI 的机器可读事件订阅面——事件分类（收件箱 / 传输 / 设备）、版本
  字段与前向兼容规则、订阅的生命周期与背压、以及无常驻节点时的行为。
- `inbox-domain-events`: 收件箱领域事件——条目新增 / 归档 / 删除在**领域内**发出一次，
  四端订阅同一个信号而不是各自从传输事件推导。含归档与软删的编排归位。

### Modified Capabilities

无。`cli-host` 与 `cli-command-surface` 的 spec 目前仍在未归档的变更里
（`changes/standalone-cli-host/specs/cli-host`、`changes/cli-command-surface/specs/cli-command-surface`），
不在 `openspec/specs/` 下；新命令对命令面既有规则的遵循写在 design，不在此处声明 delta。

`drop-inbox` 与 `inbox-store-port` 描述的是**持久化语义**，本变更不改它们的任何要求——
新增的是领域**事件**，落在新 capability 上。

## Impact

**本仓**

- `crates/cli`：新增 MCP 模块与事件订阅模块、两条命令、新依赖 `rmcp`（feature
  `server` + `transport-io`，与桌面端同为 2.x）。
- 复用现有三档取数入口（`NodeAccess` / `DaemonAccess` / `RecordAccess`）而非另起一套；
  `mcp` 归 `NodeAccess`（有常驻节点就复用、没有就自持一个，生命周期 = server 生命周期），
  知识库的分档表需补一行。
- **`crates/transfer`**：新增收件箱领域事件；`archive_inbox_item` 建编排层，
  `delete_inbox_item` 的编排函数补上事件端口。
- **四端接线**：`src-tauri`（事件转发 + 收件箱页订阅刷新）、`mobile/`（桥接镜像 + store）、
  `crates/web` + `docs/app/app`（事件类型 + store）、`crates/cli`（订阅面）。
- **两个入库产物必须重建并提交**：`packages/swarmdrop-web/`（`crates/transfer` 与
  `crates/core` 都在 `check-wasm-artifact.sh` 的 `WASM_SOURCES` 里）与 mobile 的 uniffi
  生成 TS（带 checksum 断言，需 `pnpm --filter react-native-swarmdrop-core build:ios`）。
- **`check-wasm.sh` 必跑**（动了 wasm 侧的 `transfer` / `core` / `web`）。
- 桌面与移动的 `CoreEvent` catch-all 分支**会静默吞掉新变体**（`CoreEvent` 是
  `#[non_exhaustive]`，编译器护不住这一段），两处各需一条护栏测试；移动侧有现成体例
  `file_publish_should_survive_the_mobile_mirror`。
- CLI 版本线 `cli/swarmdrop-cli-v*` 照旧，无新版本线；桌面与移动本次不发版。

> ⚠️ 本节此前写着「不动 `src-tauri`、不动 `crates/core` 与传输域、不动三端 UI」与
> 「不碰 wasm 侧七个 crate」。**那两句已作废**（2026-08-21）：调研查实三端在各自推导
> 「收件箱变了」且已经漂移，在 CLI 里再推导一遍会成为第四份、也是最差的一份。范围扩大
> 是一次明确的决定，不是漂移。

**仓外**

- 新建独立仓 `dsh-swarmdrop`（TS、npm 发布、进 awesome 列表）。它以
  `optionalDependencies` 拉 `swarmdrop` 二进制（npm 包已发布，`dist` 的 npm installer
  已就位），因此对用户是一条 `dsh plugin add`。
- 上游依赖面：dsh 处于 developer preview 且明确声明会有破坏性变更。易碎的一半全部留在
  仓外——上游改了 seam 只改那个仓，本仓的配对 / 传输 / 收件箱一行不动。
