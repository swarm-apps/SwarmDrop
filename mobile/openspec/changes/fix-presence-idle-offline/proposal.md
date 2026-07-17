## Why

已配对设备在局域网 mDNS 发现并配对后，只要没有业务流量，约 60 秒就会在对方设备列表里显示离线；执行任意 p2p 命令（如重新生成配对码）又立刻恢复在线。根因已核实到源码行号：在线状态是瞬时 libp2p 连接的镜像（无宽限期），而 swarm `idle_connection_timeout=60s` 会回收空闲连接（libp2p 0.52+ 的 ping 不再保活，`behaviour.rs:30` 的"ping…保持连接活跃"注释是过时认知）；断连后应用层零重连（`announce_online`/`check_paired_online` 只在启动时跑一次，mDNS 只对全新记录发 Discovered），只能靠 kad periodic bootstrap（5 分钟）的拨号副作用形成"离线 ~4min → 上线 ~60s"震荡。presence 是本产品"The Trusted Doorstep（可见性即信任）"的核心体验，必须让它反映真实可达性。

## What Changes

- **libs/core（swarm-p2p-core，机制层）**：新增 `KeepAliveBehaviour`（`HashSet<PeerId>` 白名单，handler 对白名单内 peer 返回 keep_alive=true）+ `SetKeepAlive { peer_id, enabled }` 命令。全局 `idle_connection_timeout` 维持 60s，陌生/瞬时连接照常回收。顺手修正 `behaviour.rs` 中 ping 保活的过时注释。
- **crates/core（swarmdrop-core，策略层）**：新增 `presence::Supervisor` 后台任务（复用 `NetworkManager` 现成的 spawn + `CancellationToken` 模式），职责：
  - per-paired-peer 状态机：`Connected → Probing（断连后退避重拨，宽限期内 UI 维持在线）→ Unreachable（低频 DHT 查在线记录 + 重拨）`；
  - 已配对 peer 全部进 keep-alive 白名单（不分直连/中继），配对/解配对时进出；
  - `announce_online` 收编为周期刷新（每 TTL/2 ≈ 150s）；
  - 死对端检测：连续 2 次 ping 失败 → 主动 disconnect → 进 Probing（若验证发现 libp2p 0.56 已自动关连接则省略此分支）。
- **presence 职责归位**：`announce_online`/`check_paired_online` 从 `pairing::manager` 搬到 `presence` 模块，pairing 只管配对。
- **BREAKING（内部，无存量兼容负担）**：桌面 `src-tauri/commands/lifecycle.rs` 与移动 `mobile-core/src/network.rs` 中一次性 `announce → bootstrap → check` spawn 整段删除，presence 生命周期由 core 自治。
- **不变**：`DeviceStatus` 枚举、`DevicesChanged` 事件形状均不变 → 两端前端 UI 零改动，移动端 uniffi bindings 无需重新生成（若最终无 FFI 面变化）。

## Capabilities

### New Capabilities
- `paired-device-presence`: 已配对设备的在线状态语义与维持机制——连接保活、断连宽限与自动重连、离线低频重探、DHT 在线记录周期刷新、死对端检测。

### Modified Capabilities

（无——现有三个 capability 均与 presence 无关，其余行为仅为实现细节调整。）

## Impact

- **桌面仓 /Volumes/yexiyue/SwarmDrop（主要改动）**：
  - `libs/core/src/runtime/behaviour.rs`（新增 behaviour 字段 + 注释修正）、`libs/core/src/command/`（新命令）、`libs/core/src/client/`（客户端 API）；
  - `crates/core/src/presence/`（新模块）、`crates/core/src/pairing/manager.rs`（presence 职责迁出）、`crates/core/src/network/manager.rs`（挂 Supervisor 任务）、`crates/core/src/device_manager.rs`（状态推导接 Supervisor 状态）；
  - `src-tauri/src/commands/lifecycle.rs`（删一次性 spawn）。
- **移动仓 /Volumes/yexiyue/SwarmDrop-RN（跟进）**：
  - `packages/swarmdrop-core/rust/mobile-core/Cargo.toml` bump swarmdrop-core git rev；
  - `mobile-core/src/network.rs`（删一次性 spawn）。
- **跨仓顺序**：桌面仓 libs/core → crates/core 先落地并推 develop，移动仓 bump rev 跟进。
- **测试影响**：E2E 中"presence 来回跳、用 uiautomator 轮询等在线"的 workaround 可在后续简化；新增 Supervisor 状态机单测。
- **能耗**：移动端已配对设备连接常驻（15s 一次 ping），Android FGS 场景下增量可忽略；iOS 后台 socket 挂起行为不变。
