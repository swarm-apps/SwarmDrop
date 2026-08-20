## Context

动机见 `proposal.md`。这里只列塑造方案的既有事实：

1. **组合根已经是平台中立的。** `swarmdrop_core::runtime::start_node(credentials, os_info,
   network_config, profile, ports, create_transfer)` 接受一个 `HostPorts`（5 个字段：
   `device_config` / `paired_device_store` / `event_bus` / `notifier` / `invite_store`），
   其中 `notifier` 是 `Option`，注释明写「`None` = 该端没有这个概念」。
   `TransferManager::new(endpoint, events, store, file_access)` 同样只要 4 个东西。
   **多宿主在设计时就被考虑过，CLI 不需要新的组合入口。**

2. **桌面与移动的「命令层」不是业务层。** 抽样对比同一用例（`prepare_send`）在
   `src-tauri/src/commands/transfer.rs` 与 `mobile-core/src/transfer.rs` 的实现：两者逐行同构，
   唯一差异是 DTO 往 specta 还是 uniffi 翻译，真正的编排只有 `manager.prepare()` 一行。
   **那 1613 + 2581 行是 FFI 边界强加的翻译层，不是可复用的业务逻辑。**

3. **端口层已有「平台中立文件实现」的先例，而且不止一处。** `crates/host/src/device_config_file.rs`
   整模块 `#[cfg(not(target_family = "wasm"))]`、用同步 `std::fs`（因为该 crate 要过 wasm
   双 target 门禁，tokio 的 `fs` feature 在 wasm 上不存在），宿主只提供路径。

   ⚠️ **实施期核实（2026-08-18，本文初稿的判断偏保守）**：桌面的 `FileAccess` 实现
   **同样已经是平台中立的**——`file_source.rs` 与 `file_sink.rs` 里全部 7 处 `tauri`
   都是**未使用的 `_app` 参数**，真正的实现住在两个 `path_ops.rs`（合计 616 行，
   零 tauri 引用，17 条测试）。也就是说需要共享的 native 实现不是两个，是**四个**。

4. **节点启停在本仓是被机器门禁看守的原则。** `pnpm check:node-lifecycle` 禁止在
   `useEffect` 里调节点启停，理由是「那会长成收敛环，用户点了停止立刻被拉回」。

5. **分发有可直接参照的既有配置。** `../SwarmHive/dist-workspace.toml` 已在生产使用
   `dist`，并记录了 tag 形态陷阱与 workflow 隔离方式；`swarm-apps/homebrew-tap` 已存在。

## Goals / Non-Goals

**Goals**

- CLI 只做两件事：**凑齐端口**、**提供命令面**。不新增任何业务层。
- 复用优先于复制：能下沉的实现下沉，不能下沉的（FFI 翻译）不强行抽象。
- IPC 保持为内部机制，形状由 CLI 自己的命令面决定，不提前迎合外部消费者。

**Non-Goals**

- **不抽「应用服务层」。** 见 Context 2：那层重复是 FFI 造成的，抽出来对 CLI 无收益，
  对现有两端是一次没有回报的大重构。
- **不与同机图形界面宿主共享身份或数据库。** 见 D3。
- 不做 Agent 端点、不做 CLI 版 MCP server。

## Decisions

### D1：CLI 直接消费 core 类型，不建 DTO 层

CLI 不跨 FFI，`crates/core` / `crates/transfer` 的类型可以直接用。命令层的职责退化为
「解析参数 → 调 manager → 渲染输出」。

**备选**：为 CLI 定义一套自己的 DTO，以隔离 core 的类型变动。**否决**——那正是桌面与移动
被迫做的事，CLI 没有被迫的理由；多一层翻译只会多一处漂移点。

### D2：独立设备身份，而非与桌面端共享

**否决的备选**：共享 identity 与 `paired-devices.json`，靠文件锁选举出唯一的节点持有者，
其余进程降级为瘦客户端（含桌面端）。

否决理由是一条推导链：**共享身份 ⇒ 共享 SQLite ⇒ 必须单进程独占 DB ⇒ 桌面端也得成为
IPC 服务端**。也就是说，共享身份的代价不是「加一层 IPC」，而是**改造一个已发布的产品**，
且这套复杂度在本 change 的验收路径（一台没装过 SwarmDrop 的机器）上一次都用不到。

代价如实记录：同机的 CLI 与桌面端是两台设备，各自的收件箱与传输历史不互通，用户需要
分别配对。默认设备名带上可区分的后缀，让这件事在设备列表里是**诚实可见**的而非隐藏的。

**该决策可后向迁移**：日后若要共享，是在既有 CLI 前面加一层前置判断，不需要推翻本设计。

### D3：`start`/`stop` 而非隐式的「持锁者即 daemon」

**否决的备选**：不提供 `start`，任何命令抢到锁就成为当次的节点持有者。

否决理由有三：核心的 `NodeStatus` 本来就只有 `Running` / `Stopped` 两态；三端 UI 都提供
显式启停；而 `check:node-lifecycle`（Context 4）已经把「启停必须显式」定为本仓原则，
隐式启动是它防的东西的 CLI 版本。用户也无法预测哪条命令会突然变成常驻进程。

**同时删除 `recv` 命令**：接收是节点在线的被动结果，做成动作会与三端的产品模型分叉。

### D4：两种节点生命周期，一套仲裁

| | 常驻节点 | 临时节点 |
|---|---|---|
| 谁起的 | `start` | 无节点时的一次性命令 |
| 何时止 | `stop` 或终止信号 | 命令结束 |
| 持锁 | 是 | 是 |
| 开 IPC 通道 | 是 | 是 |

两者都持锁、都开通道，是为了让**「有没有通道」== 「有没有节点」**这条判断保持单一。
`status` 不区分二者（设备此刻确实在线），`stop` 对两者都生效（用户的显式意图优先于
正在跑的命令）。

### D5：通道用于发现，文件锁用于仲裁

单实例判定**不用 pidfile**：PID 会被复用，陈旧 pidfile 会误判为「有节点在跑」。

改为两段：
1. **发现**——通道存在且能连上 ⇒ 有活节点，走 IPC；连不上 ⇒ 判为陈旧残留。
2. **仲裁**——判为陈旧后不能直接启动：两个进程可能同时做出该判断。以数据目录上的
   文件锁做最终裁决，拿到锁者清理残留并启动，未拿到者回到第 1 步重连。

### D6：IPC 是内部机制，动词集与命令面一一对应

传输用 Unix domain socket（类 Unix）与命名管道（Windows），载荷用长度前缀的结构化消息。

**不提前把它设计成对外 API。** 动词集就是命令面的映射（devices / send / status / inbox / stop），
两端都是本 crate 的代码，可以随时改。Agent 端点若要对外暴露能力，那时再基于真实需求
决定是提升这套通道还是另起一个面——现在为一个未定的消费者做通用化是投机。

### D7：事件订阅写一遍，渲染分三种

`CoreEvent` 是一个大枚举，订阅与分发逻辑与输出形态无关。因此**不做三个 `EventSink` 实现**，
而是一个 sink 持有一个渲染器：

| 渲染器 | 目标 | 用于 |
|---|---|---|
| 人类可读 | stderr（进度条 / 状态行） | 交互式一次性命令 |
| 结构化日志 | stderr | `start` 常驻 |
| 机器可读 | **stdout** | `--json` |

分流到 stderr / stdout 是硬约束：结构化模式下 stdout 只能有最终结果，任何进度信息混入
都会破坏调用方的解析。

### D8：native 文件实现独立成 `crates/host-fs`，`crates/host` 回归纯端口

本文初稿写的是「下沉到 `crates/host`」，并留了一句拆分触发条件：

> 目前只有两个文件实现，为此多一个 crate 的收益不足……若第三、第四个实现出现，再拆不迟。

**第三、第四个在实施期就出现了**（见 Context 3 的核实）。因此改为：

```
crates/host        纯端口：trait + DTO + error + device 类型。零文件 IO。
crates/host-fs     native 本地文件系统实现（cfg(not(wasm))）：
                     JsonFileIdentityStore     KeychainProvider + PairedDeviceStore
                     JsonFileDeviceConfig      DeviceConfig
                     file source / file sink   FileAccess 的本地路径实现
```

**依赖方向是这次调整的重点**：

| crate | 依赖 | 理由 |
|---|---|---|
| `crates/core` | 只依赖 `crates/host`（端口） | **绝不依赖 `host-fs`**——core 要过 wasm 门禁，而 host-fs 是 native-only |
| `src-tauri` / `crates/cli` / `mobile-core` | 直接依赖 `host-fs` | 宿主自己选实现，这正是端口与实现分离的正确形态 |

副作用是宿主不再能从 `swarmdrop_core::host::` 拿到这些实现（core 不再传递它们），
必须显式依赖 `host-fs`。**这是好事**：谁用实现、谁就声明它，依赖图上一眼看得出。

移动端的位置要说清楚：它依赖 `host-fs` 只为 `JsonFileDeviceConfig`；
`FileAccess` 那部分它**用不上**（Android SAF 与 iOS 的落点语义完全不同，自有实现）。
门控因此只做 `cfg(not(wasm))` 而不细分到 OS——移动端编译它但不使用，多出的产物可忽略，
而按 OS 细分会引入一个需要长期维护的 `cfg` 矩阵。

**为什么值得动桌面端的传输热路径**：`FileAccess` 的契约里记录了两次真实事故
（offset 取整到 chunk 边界导致 prepare panic 进 blake3；`open_or_create_sink` 转调
`create_sink` 导致续传截断、产出「长度正确但内容有洞」的文件）。让 CLI 重写一份，
等于重新制造踩这两个坑的机会；复用一份带 17 条测试、经生产验证的实现，风险低得多。
**迁移的实际形状**（实施期核实后修正——初稿写的「纯搬移」不准确）：桌面那条链是三层，
`TauriFileAccess` → `FileSource`/`FileSink` 的 enum 分派 → `path_ops` 纯函数，而中间那层
**两个 enum 各自只剩一个变体**（`Path`），是历史上存在多种来源类型时留下的。

因此迁移**跨过中间层**：`host-fs` 直接提供一个 `LocalFileAccess`（持 `active_sinks`
+ 复用 `path_ops`），桌面按职责拆两半——

| 留在桌面 | 迁入 `host-fs` |
|---|---|
| `FileSource` / `EnumeratedFile`（带 `specta::Type`，跨 IPC 给前端） | 两个 `path_ops`（616 行 + 17 条测试） |
| `enumerate_dir` / `source_id`（服务 `scan_sources` 命令，**不属于 `FileAccess` 契约**） | `PartFile` / `compute_part_path` |
| — | `LocalFileAccess`（取代 `TauriFileAccess`） |

判据是**契约归属**：`FileAccess` trait 要求的能力迁走，桌面 IPC 的 DTO 与扫描命令留下。
`FileSourceId` 本质是文件路径字符串（外加一个 JSON 格式的向后兼容分支），所以跨过 enum
分派不丢任何信息。行为逐字不变，17 条测试随实现一起走。

### D9：分发配置对齐 SwarmHive，隔离靠 tag-namespace

- `tag-namespace = "cli"` ⇒ 产出独立命名的发布 workflow，**不覆盖既有的 Tauri `release.yml`**。
- 发布标签用**斜杠**形式 `cli/v0.1.0`。SwarmHive 的配置注释记录了这个陷阱：
  namespace 与包名不同时，连字符形式会被整串当作版本号解析而失败——本 change 同样是
  namespace `cli` ≠ 包名，因此同样必须用斜杠。
- 复用 `swarm-apps/homebrew-tap`，formula 名取干净的二进制名。
- `install-updater = false`：更新由安装渠道负责，内建自更新会与包管理器争夺版本来源。

### D10：模块划分

```
crates/cli/
├── main.rs            入口：参数解析 → 分派
├── cmd/               每条子命令一个模块，只做「解析 → 调用 → 渲染」
├── runtime/
│   ├── boot.rs        凑齐 HostPorts → start_node
│   ├── single.rs      D5 的发现 + 仲裁
│   └── ipc/           通道的客户端与服务端
├── adapter/
│   ├── events.rs      EventSink（D7）
│   ├── files.rs       FileAccess（纯 std::fs）
│   └── paths.rs       数据目录解析
└── render/            人类可读 / 机器可读两套渲染
```

分层判据：`cmd/` 不含网络与存储细节；`runtime/` 不含任何面向用户的文案；
`render/` 不含业务判断。

### D11：TUI 不进本 change，但为它保留边界

`ratatui-kit`（自有框架，crates.io `0.10.x`，活跃维护）是后续做交互式终端界面的现成底座。
本 change **不做 TUI**，理由三条：它不在本 change 的验收路径上；CLI 二进制要经 npm 分发给
agent harness 消费，而那个场景下 TUI 依赖是纯负担；且 TUI 不是「另一种渲染」而是**另一个入口**
——一次性命令与有状态的全屏应用有各自的交互模型，前者的 `cmd/` 与 `render/` 对后者不可复用。

**本 change 需要承担的只有一条边界**：`runtime/` 层 MUST NOT 假设调用方是一次性命令。
节点装配、单实例仲裁与通道客户端都要能被一个长期存活的交互式前端复用。
满足这条，TUI 就是新增一个入口 + 一套渲染，不需要回头重构。

若日后接入，判据留在这里：作为默认关闭的可选特性，或独立二进制——**不得让默认的
npm 分发产物背上 TUI 依赖**。

## Risks / Trade-offs

- **同机数据割裂**（CLI 与桌面端各自的收件箱与历史）→ 默认设备名可区分，使其可见而非隐藏；
  D2 的迁移路径保持开放。
- **一次性命令的冷启动成本**：`net-kernel.md` 记录打洞需「等 ICE 收敛数秒」，而对端常在 NAT 后。
  → `start` 常驻可完全规避；文档需说明「频繁发送就先 start」。**端到端耗时尚未实测**（见 Open Questions）。
- **`dist` 再生成覆盖既有 workflow** → 先设 `tag-namespace` 再生成；把「既有 `release.yml`
  内容不变」列为验收项。
- **Windows 的通道与后台化路径与类 Unix 完全不同** → 通道用跨平台抽象；后台化能力若在
  Windows 上代价过高，宁可只保留前台（前台是服务管理器与外部程序托管的必要形态，后台是便利）。
- **`crates/host` 继续变重**（端口层放第二个文件实现）→ 已在 D8 记录拆分触发条件。

## Migration Plan

无数据迁移：CLI 是新增宿主，使用自己的数据目录。

`identity_store` 下沉是**纯重构**：桌面端行为逐字不变，护栏测试随实现迁移并保持通过。
若下沉过程中发现桌面实现里混有平台相关逻辑，该部分留在桌面侧，只下沉确属中立的部分——
**不要为了下沉而把平台细节塞进端口层**。

回滚：CLI 是独立 crate 与独立版本线，出问题不影响桌面与移动的发布路径；
下沉部分可单独回退为桌面本地实现。

## Open Questions

- **npm 包的 scope 与名称**。需要一个可发布的 npm 组织；这决定 Agent 端点那侧
  `optionalDependencies` 的写法，但不影响本 change 的规格与任务拆分。
- **TUI 何时接入、以何种形态打包**（可选特性 vs 独立二进制）。D11 已定下不进本 change 与不可
  违反的边界，具体形态取决于届时的二进制体积实测，不影响本 change。
- **冷启动端到端耗时**（从进程启动到对一台离线过的设备首字节）。它只影响文档口径与
  是否要在后续提供连接预热，不改变本设计。测量不依赖 CLI，桌面端加计时日志即可。
