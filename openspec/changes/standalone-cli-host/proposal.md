## Why

SwarmDrop 有桌面、移动、Web 三个 GUI 宿主，**唯独没有命令行形态**——而这恰恰是开发者
最容易接受、分发摩擦最低的那一个（对照：croc 39.8k★、magic-wormhole 22.8k★）。

更直接的驱动来自下一步的 Agent 端点（让 AI agent 成为可配对的对端）：任何 harness
（dsh / Claude Code / Codex）的扩展机制互不相同，**唯一的公约数是「能执行一条命令」**。
没有 CLI，接入 harness 就只能要求用户先装桌面 GUI；有了 CLI，接入退化为
`npm optionalDependencies` 拉一个二进制。

顺带兑现一份既有规划：`dev-notes/architecture/future-openspec-candidates.md` §5
`mcp-cross-host`（「让 MCP server 不再绑死在桌面端，能在独立 CLI 上以 sidecar 形式运行」）。

完整论证见 `dev-notes/research/2026-08-18-agent-endpoint-proposal.md`。

## What Changes

- **新增 `crates/cli`（binary `swarmdrop`）**：第四个宿主，复用 `swarmdrop_core::runtime::start_node`
  这个既有的平台中立组合根，自备 headless 的 `EventSink` 与 `FileAccess` 实现。
- **节点生命周期与三端对齐**：`start` / `stop` / `status` 直接对应核心已有的
  `NodeStatus::{Running, Stopped}`，与桌面 `NodeStatusSheet`、移动 `NodeControlSheet`、
  Web 节点弹窗是同一语义的第四份实现。**不引入隐式启动**——本仓已用
  `pnpm check:node-lifecycle` 把「节点启停必须是显式动作」钉死在前端，CLI 不得开倒车。
- **不提供 `recv` 命令**：接收是节点在线时的被动后台行为，与三端的「配对 + 被动接收」
  模型一致；把接收做成一个动作会让 CLI 与其余三端分叉。
- **两种节点生命周期**：`start` 起常驻节点（活到 `stop`）；节点未运行时的一次性命令
  （如 `send`）起**临时节点，命令结束即销毁**——既不让用户干等，也不违背用户的 `stop` 意图。
- **本地 IPC（最小动词集）**：常驻节点存在时，其余命令经本地 socket 复用它。没有它，
  `start` 与 `send` 会因同一身份两进程而硬冲突（DHT 记录互覆盖、relay reservation 互踢）。
- **CLI 使用独立设备身份**（默认 `<hostname> (cli)`），与同机桌面端**不共享** identity 与
  数据库。共享身份会连带要求共享 SQLite 与改造已发布的桌面端，代价与 M1 目标不匹配。
- **native 文件实现独立成 `crates/host-fs`，`crates/host` 回归纯端口**：桌面的
  `identity_store`（原子写 + 0600 + 读失败不降级）与 `FileAccess`（`file_source` /
  `file_sink` 的两个 `path_ops`）本就是平台中立的——后者全部 7 处 `tauri` 都是未使用的
  `_app` 参数。四个实现（身份存储 / 设备配置 / 文件读写）集中到一个 native-only 的实现
  crate，原生宿主各自只留「路径怎么算」。**core 只依赖端口、不依赖实现**，宿主自己选实现。
- **`dist`（原 cargo-dist）分发全套**：shell / powershell / npm / homebrew 四种 installer，
  复用已有的 `swarm-apps/homebrew-tap`。经 `tag-namespace` 产出**独立命名的
  workflow**，不触碰既有的 Tauri `release.yml`。新增第三条版本线 `cli/v*`。
- **`pair` 在终端渲染二维码**（附 `--no-qr` 开关）：手机扫码是配对主路径，
  base32 邀请串手输不现实。

**非目标**：不动桌面端与移动端的任何既有行为；不做 Agent 端点（下一个 change）；
不做 MCP server 的 CLI 版（`mcp-cross-host` 的另一半，待 Agent 端点定型后再议）。

## Capabilities

### New Capabilities

- `cli-host`: CLI 作为第四宿主的完整契约——命令面、`start`/`stop`/`status` 的节点生命周期
  语义、常驻节点与临时节点两种生命周期、本地 IPC 的存在条件与动词集、独立设备身份、
  三种输出模式（人类可读 / 结构化日志 / `--json`）、退出码约定。
- `cli-distribution`: CLI 的分发契约——`dist` 配置、`cli/v*` 版本线与既有两条版本线的隔离、
  installer 与平台矩阵、生成的 workflow 必须与 Tauri `release.yml` 互不触发。

### Modified Capabilities

- `host-identity-storage`: 新增一条要求——身份与已配对设备的**文件实现是所有原生宿主共享的
  同一份**（含原子写、unix 0600、读取失败不降级等既有保证），宿主只提供路径。
  原有的桌面/RN scenario 行为不变。
- `host-file-access`: 新增一条要求——本地文件系统的 `FileAccess` 实现同样是原生宿主共享的
  同一份，宿主只提供保存位置。既有的读取契约与暂存/发布语义逐字不变。

## Impact

**新增**
- `crates/cli`（bin `swarmdrop`）——新 workspace 成员
- `crates/host-fs`——native 本地文件系统实现（身份存储 / 设备配置 / 文件读写），
  `#[cfg(not(target_family = "wasm"))]`
- `dist-workspace.toml`、`.github/workflows/cli-release.yml`（由 `dist generate` 产出）

**修改**
- `crates/host`：**移出**全部文件实现，回归纯端口（trait + DTO + error + device 类型）
- `src-tauri/src/host/`：`identity_store` / `device_config` / `file_source` / `file_sink`
  改为复用共享实现 + 提供路径，**行为逐字不变**；未使用的 `_app` 参数一并删除；
  相关护栏测试（原子写、读取失败不降级、读取契约、暂存/发布、符号链接越界）随实现迁移
- `mobile/packages/.../mobile-core`：改为直接依赖 `host-fs` 取 `JsonFileDeviceConfig`
  （此前经 core 的 re-export 拿到）
- `CLAUDE.md`：Version management 小节增补第三条版本线；Workspace 布局表增补 `crates/cli`
- 根 `Cargo.toml`：workspace members

**不修改**
- `crates/core` / `crates/transfer` / `crates/net`：CLI 是纯消费方，组合根 `start_node`
  与 `TransferManager::new` 的签名不需要变动。**core 刻意不依赖 `host-fs`**——它要过
  wasm 门禁，而实现是 native-only；宿主自己选实现
- 桌面端与移动端的任何用户可见行为

**风险**
- `dist generate` 若未正确设置 `tag-namespace`，会覆盖既有 `release.yml`（Tauri 发版流水线）。
  参照 `../SwarmHive/dist-workspace.toml` 的既有配置与其中记录的 tag 形态陷阱
  （namespace ≠ 包名时必须用 `cli/v0.1.0` 斜杠形式，连字符会被当版本号解析而失败）。
- CLI 与同机桌面端数据割裂（各自的收件箱与传输历史）——本 change 接受该代价，
  记录为已知限制。
