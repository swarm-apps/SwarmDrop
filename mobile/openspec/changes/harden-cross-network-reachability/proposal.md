## Why

LAN Helper 网关拓扑（手机 A=局域网协作节点+公网中继客户端，电脑 B=LanOnly，跨网设备 C）下，presence 与可达性在 v0.7.6 后仍有结构性缺陷（12 agent 调查+逐环核实，证据见 design）：**relay reservation 一次性建立、断线永久失效且全代码库无重建路径**（三层一次性门禁），A 挂起一次即导致 B 跨网永久不可达；**C→B 方向结构性拨不通**（B 发布的在线记录只含私网地址、锚定 A 私网 IP 的 circuit 地址和 libp2p 硬拒的非法二跳地址），C 看 B 在线完全寄生于 B 反向拨入；**A 的中继限额 64MiB/30min 必掐大文件传输**。这些洞与 0.7.5 presence bug 同病：命令式一次性编排、无声明式收敛——而 `presence::Supervisor` 已确立正确模式，本 change 把它推广到基础设施层。

## What Changes

- **libs/core（机制）**：
  - 新增 `NodeEvent::RelayReservationLost`（circuit listener 关闭时上抛，现仅 warn 日志）+ event loop 内 reservation 状态跟踪（listener↔relay peer 映射）；
  - 新增幂等原语 `ensure_relay_reservation(peer, addrs)`（有活跃 reservation 则 no-op）；
  - `RelayLimits` 默认调整：`max_circuit_bytes=无限`、`max_circuit_duration=12h`（自己的设备给自己转发，64MiB/30min 无意义）。
- **crates/core（策略）**：
  - 新增 `infra::Supervisor`（收敛家族第二员，与 presence::Supervisor 同模式独立任务）：对每个基础设施候选维持「连接存活 + kad 接线 + reservation 存活（允许时）」，断线退避重建；`BootstrapCandidateManager` 升级为期望状态唯一 source of truth；
  - **删除三层一次性门禁**（candidates.upsert changed 门禁、bootstrap_peers 一次性 remove、ListenerClosed 只打日志），防重复职责移交幂等原语；
  - 学习型候选：IdentifyReceived 识别 `is_bootstrap_agent` 的公网中继 → 纳入候选表（来源 Learned）——B 在 LanOnly 下经 A 认识公网中继即自动纳管；
  - `OnlineRecord` **BREAKING** 结构化：`direct_addrs`（过滤 loopback/unspecified/非法二跳）+ `relay_addrs`（合法一跳 circuit）+ `relays` hint（relay PeerId+可达地址）；
  - announce 事件驱动：地址集/reservation 变化 → 立即重发，150s 周期降为兜底；
  - `spawn_probe` 多步编排：直拨失败且有 relay hint → 先确保与 relay 直连再拨 circuit；失败原因分级上报替代静默。
- **设置语义拆分（BREAKING）**：`discovery_mode` 保持只管"连不连内置公网引导"；新增 `public_reachability: bool`（默认 true）管"允不允许经公网中继被访问"——LanOnly+默认开 = 自动在学到的公网中继上 reservation，C 一跳直拨 B，零打洞依赖；严格隔离 = LanOnly+关。
- **UI（P5）**：本机网络状态展示"公网可达/仅局域网可达"（`NetworkStatus` 加 `public_reachable`）；LanOnly 设置文案改写真实语义；设备列表 presence 保持二元不动。
- **不做**：BOOT identify→kad 接线（P6，直连 reservation 后重要性下降）；kad 记录持久化（MemoryStore 单点留 follow-up）。

## Capabilities

### New Capabilities
- `infra-connectivity`: 基础设施链路（bootstrap/LAN Helper/学习型中继）的连接、kad 接线与 relay reservation 的声明式收敛维持，含中继限额的文件传输适配。
- `public-reachability`: 公网可达性的设置语义（discovery 与 reachability 解耦）与可达状态可见性。

### Modified Capabilities
- `paired-device-presence`: 「在线宣告周期刷新」升级为结构化记录+事件驱动刷新；「离线设备低频重探」升级为 relay 感知的多步编排。（注意：该 capability 尚在 `fix-presence-idle-offline` 中待归档，**归档顺序必须 fix-presence-idle-offline 在前**。）

## Impact

- **桌面仓 /Volumes/yexiyue/SwarmDrop**：`libs/core`（event.rs/runtime/event_loop.rs/config.rs/新命令）；`crates/core`（新 `infra/` 模块、`network/candidates.rs`、`network/event_loop.rs` 门禁删除、`presence/`（record/announce/probe）、`network/config.rs`+`NetworkStatus`）；`src-tauri` 设置与状态 UI。
- **移动仓 /Volumes/yexiyue/SwarmDrop-RN**：bump rev；`NetworkRuntimeConfig`/`NetworkStatus` 镜像加字段（穷尽解构 drift guard 会编译报错逼同步）→ **本次需要重生成 bindings**（走轻量重生成路径）；设置页加公网可达开关与状态展示。
- **测试**：infra::Supervisor 收敛单测；reservation 断线重建集成测试；跨网拓扑真机验证依赖用户环境。
- **发版顺序**：libs→crates→桌面 host/UI→推 develop 取 rev→移动仓跟进。
