## Context

在线状态目前是瞬时 libp2p 连接的纯镜像：`DeviceManager` 由 `PeerConnected`/`PeerDisconnected` 翻转 `is_connected`，Paired 视图下 `!is_connected` 即 Offline，无宽限期、无 last-seen（`crates/core/src/device_manager.rs:173-177`）。swarm `idle_connection_timeout=60s`（`libs/core/src/config.rs:179`）回收空闲连接，而 libp2p 0.52+ 的 ping 流被 `ignore_for_keep_alive()` 排除、不再保活（`behaviour.rs:30` 注释是过时认知）。断连后无人重连：`announce_online`（DHT 记录 TTL 300s）与 `check_paired_online`（显式 dial）只在两端 host 启动时各跑一次；mDNS 只对全新记录发 Discovered→dial，双方存活时记录持续刷新永不重发。唯一"意外重连"是 kad periodic bootstrap（5 分钟）的拨号副作用，形成"离线 ~4min → 上线 ~60s"震荡。

约束：libs/core（swarm-p2p-core）是不含业务概念的通用 p2p 库；"已配对"是 crates/core 的业务概念。`NetworkManager` 已有后台任务模式（spawn + `CancellationToken` + `cancel_background_tasks()`，`network/manager.rs:78,119`）。无存量用户，不留兼容层（用户既有偏好：治本优先）。

## Goals / Non-Goals

**Goals:**
- 已配对设备闲置时 presence 稳定为真实可达性：局域网内常绿，远端回归 1 分钟级被发现，消除 5 分钟震荡。
- 三个方向合一实现：保活（预防）+ 自动重连（恢复）+ 宽限语义（表达），收敛为「一个机制 + 一个大脑」。
- presence 职责归位：从 `pairing::manager` 与两端 host 收编进 core 单一模块。
- 对外零变化：`DeviceStatus` 枚举、`DevicesChanged` 事件形状、两端 UI、uniffi bindings 均不动。

**Non-Goals:**
- 不做第三态"重连中" UI（Probing 期间维持 Online，二元展示）。
- 不做 last_seen 持久化/「最后在线 x 分钟前」展示（`discovered_at` 字段维持现状，留待后续产品决策）。
- 不改 iOS 后台挂起行为（后台保活是另一个 change 的范畴）；不做移动端前台恢复时的主动 nudge（状态机对事件天然收敛，先观察是否需要）。
- 不动全局 `idle_connection_timeout`（陌生/瞬时连接照常 60s 回收）。

## Decisions

### D1: 机制/策略分层 —— libs/core 白名单 KeepAliveBehaviour，crates/core 决定谁进白名单

libs/core 新增 `KeepAliveBehaviour`：内部 `HashSet<PeerId>` 白名单，`ConnectionHandler::connection_keep_alive()` 对白名单内 peer 返回 true；新命令 `SetKeepAlive { peer_id, enabled }`（命令模式现成，`command/dial.rs` 旁加一个文件）。

- 弃选「调大全局 idle timeout」：语义粗糙，陌生连接、bootstrap 查询连接全都不回收；且没有表达"已配对设备值得常驻连接"的意图。
- 弃选「keep-alive 逻辑放 crates/core」：swarm behaviour 必须在 swarm 构建处（libs/core）注册，业务层无法事后注入 handler；通过命令控制白名单正是既有 client→command→event_loop 模式。
- 保活范围 = **所有已配对 peer**，不分直连/中继（已拍板）：个人设备网格规模下成本可忽略，DCUtR 大多升直连，行为统一无新谜题。

### D2: presence::Supervisor 作为唯一的 presence 大脑（crates/core 新模块）

单一组件消费三类输入——连接事件（PeerConnected/PeerDisconnected）、ping 事件（PingSuccess/PingFailure）、定时器（announce 刷新 / Unreachable 重探）——驱动 per-paired-peer 状态机：

```
Connected ──断连 / 连续2次ping失败──▶ Probing(退避重拨, 宽限期≈15s)
    ▲                                   │拨通            │宽限超时
    └───────────────────────────────────┘                ▼
                                              Unreachable(每75s±抖动: DHT查在线记录+重拨)
```

- 接线方式仿 `DeviceManager`：core 事件循环（`crates/core/src/network/event_loop.rs`）同步调 `supervisor.handle_event(&event)` 折叠状态；退避重拨/定时刷新跑在 Supervisor 自己的 tokio 任务里，挂 `NetworkManager` 现成的 `CancellationToken`。
- 状态推导：`DeviceManager::get_devices` 对 Paired 设备改读 Supervisor 的 presence 状态（共享 `DashMap<PeerId, PresenceState>`），`Online = Connected | Probing`，`Offline = Unreachable`。DeviceStatus 枚举不变。
- 弃选「逻辑散在 event_loop/device_manager/pairing 各处」：正是现状的病根——一次性调用散落两端 host、pairing 里混着 presence 职责。
- 弃选「host 层定时器」：桌面/移动要各写一份，重复且移动端还受生命周期干扰。

### D3: presence 职责从 pairing::manager 迁出

`announce_online` / `check_paired_online` / `announce_offline` 搬进 `presence` 模块；`pairing::manager` 只留配对码与配对流程。`check_paired_online` 的逻辑被 Supervisor 吸收：启动时对所有已配对设备做一轮 DHT 查询 + dial（即 Unreachable 重探的首轮），不再是独立入口。

### D4: 宽限期语义（Probing 期间 UI 维持 Online）

断连 ≠ 离线。进入 Probing 后立即重拨，退避序列约 0s/2s/5s/10s，宽限期 ~15s 内拨通则用户全程无感；超时才翻 Unreachable 并推 `DevicesChanged`。WiFi 漫游、网络切换的秒级抖动不再闪灰。弃选第三态"重连中"：utility app 的高频噪音，且枚举变更要跨仓贯穿两端 UI + 重生成 bindings，收益不成比例。

### D5: 在线宣告周期化 + 死对端检测

- `announce_online` 由 Supervisor 每 TTL/2（≈150s）刷新，节点存活期间 DHT 在线记录永不过期；`shutdown` 时 `announce_offline` 行为保留。
- 死对端：连接常驻后空闲回收不再兜底，Supervisor 数连续 2 次 `PingFailure`（≈30-40s）→ 主动 `disconnect` → 进 Probing。**前置验证**（见 Open Questions）：若 libp2p 0.56 的 ping 失败或 QUIC 传输层 idle 已自动关连接，此分支直接省略，靠 PeerDisconnected 进状态机。

### D6: 跨仓落地顺序与兼容策略

桌面仓 libs/core → crates/core → src-tauri 先落地推 develop；移动仓 bump `swarmdrop-core` git rev + 删 host spawn 跟进。两端 host 的一次性「announce → bootstrap → check」spawn 整段删除（**BREAKING**，内部无兼容负担）；`bootstrap()` 调用保留（收进 core 启动序列或 Supervisor 首轮）。

## Risks / Trade-offs

- [libp2p 0.56 ping 失败语义不明] → 实现前先写一个最小验证（读 libp2p-ping/libp2p-quic 源码或双进程实测拔网线），确认后决定 D5 的死对端分支去留；避免做重复机制。
- [Probing 期间显示在线但实际已死 → 用户此刻发文件会失败] → 窗口仅 ~15s；传输路径本就有连接失败的错误处理与重试语义，失败提示兜底。
- [常驻连接的能耗/资源] → 移动端 15s ping 的增量在 Android FGS 场景可忽略；中继电路常驻在个人设备网格规模下可接受，DCUtR 通常升直连。若未来设备数上百再引入按需策略。
- [mDNS Expired 仍无人处理，死地址残留地址簿] → 非本 change 根因（Expired 不参与离线判定）；重拨对死地址的重试有退避与宽限期封顶，影响有限。记为后续清理项。
- [Supervisor 与 event_loop 的并发正确性] → 状态折叠在事件循环线程同步完成（同 DeviceManager 模式），定时任务只通过 client 命令与 DashMap 交互，无跨任务锁序问题。
- [移动端 iOS 前台恢复的收敛速度] → 挂起期间连接已死，恢复后依赖 PingFailure/PeerDisconnected 进 Probing，最坏 ~1 分钟收敛；若实测体感差，再加前台 nudge（已列 Non-Goal，留观察）。

## Migration Plan

1. 桌面仓：libs/core 加 behaviour + 命令 → crates/core 加 presence 模块、迁移 pairing 职责、接 device_manager → src-tauri 删 spawn → 单测 + 桌面↔桌面实测闲置 10 分钟。
2. 桌面仓推 develop，取得 rev。
3. 移动仓：bump rev → 删 mobile-core spawn → `cargo build` 确认无 FFI 面变化（无需重生成 bindings）→ iOS sim ↔ Android emu 局域网实测。
4. 回滚策略：单 change 原子提交，revert 即回到现状（震荡但可用）。

## Open Questions

- ~~libp2p 0.56 中 ping 连续失败是否已触发连接关闭？QUIC 传输层 `max_idle_timeout` 在 swarm keep-alive 豁免下是否仍生效？~~ **已验证（2026-07-05）**：
  - ping 失败只上报事件不关连接（libp2p-ping-0.47.0 handler `ToBehaviour = Result<Duration, Failure>`，0.52+ handler 已无主动 Close 能力）；且 handler 静默吞掉第 1 次失败、从第 2 次连续失败起才上报（handler.rs:272-287）——阈值 2 个事件 ≈ 协议层连续 3 次失败 ≈ 40s，+15s 宽限 ≈ 1 分钟判离线，与 spec 场景一致；
  - QUIC 传输层默认 `max_idle_timeout=10s` + `keep_alive_interval=5s`（libp2p-quic-0.13.0/src/config.rs:92,94），死对端 10 秒自动判死关连接，且不受 swarm keep-alive 豁免影响（豁免只管空闲回收）；
  - TCP 无传输层 keepalive，死对端不会自动断。
  - **裁决**：保留 Supervisor 连续 2 次 ping 失败 → 主动 disconnect 的分支，作为 TCP 连接的兜底（QUIC 路径由传输层先行判死，不会走到阈值）。libs/core 新增 `NodeEvent::PingFailure` 事件承载失败信号。
- 退避序列与宽限期的最终参数（初值 0/2/5/10s、宽限 15s、重探 75s±抖动）以实测手感微调，spec 只约束数量级。
