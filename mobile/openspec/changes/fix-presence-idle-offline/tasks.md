## 1. 前置验证（决定 D5 死对端分支去留）

- [x] 1.1 核实 libp2p 0.56 中连续 ping 失败是否自动关闭连接（读 libp2p-ping 0.47 handler 源码的 Failure 分支 + libp2p-quic `max_idle_timeout` 默认值），结论写回 design.md Open Questions
- [x] 1.2 若传输层/ping 已自动关连接：删除 specs 中"死对端检测"的自实现路径（改为由该机制满足）；否则确认 Supervisor 数 ping 失败方案不变

## 2. 桌面仓 libs/core（机制层）

- [x] 2.1 新增 `keep_alive` behaviour：`HashSet<PeerId>` 白名单 + 对白名单 peer 返回 keep_alive=true 的 ConnectionHandler，注册进 `CoreBehaviour`（`libs/core/src/runtime/behaviour.rs`）
- [x] 2.2 新增 `SetKeepAlive { peer_id, enabled }` 命令（`libs/core/src/command/set_keep_alive.rs`，仿 dial.rs 模式）+ client 侧 API `set_keep_alive()`
- [x] 2.3 修正 `behaviour.rs:30` 附近"ping…保持连接活跃"的过时注释（改为说明 ping 仅测 RTT/存活，不保活；保活由 keep_alive behaviour 承担）
- [x] 2.4 libs/core 单测：白名单内 peer 空闲不被回收、白名单外 60s 回收、动态增删白名单生效

## 3. 桌面仓 crates/core（策略层：presence::Supervisor）

- [x] 3.1 新建 `crates/core/src/presence/` 模块：`PresenceState`（Connected/Probing/Unreachable）+ 共享 `DashMap<PeerId, PresenceState>` + Supervisor 骨架（挂 `NetworkManager` 的 spawn + CancellationToken 模式）
- [x] 3.2 迁移 presence 职责：`announce_online`/`announce_offline`/`check_paired_online` 从 `pairing::manager` 搬入 presence 模块，pairing 只留配对码与配对流程（调用点同步更新）
- [x] 3.3 事件折叠：core 事件循环（`crates/core/src/network/event_loop.rs`）接入 `supervisor.handle_event(&event)`，处理 PeerConnected/PeerDisconnected/PingSuccess/PingFailure（仅已配对 peer）
- [x] 3.4 Probing 状态机：断连（或按 1.x 结论的连续 2 次 ping 失败→主动 disconnect）→ 退避重拨 0/2/5/10s、宽限 ≈15s；宽限内不推离线，拨通回 Connected，超时转 Unreachable 并推 `DevicesChanged`
- [x] 3.5 Unreachable 低频重探任务：每 75s±抖动对离线已配对设备查 DHT 在线记录 + add_peer_addrs + dial；节点启动首轮覆盖全部已配对设备（吸收原 check_paired_online）
- [x] 3.6 announce_online 周期刷新任务：启动即发一次，此后每 ≈150s（TTL/2）重发；shutdown 保持 announce_offline
- [x] 3.7 保活白名单接线：启动时装载全部已配对设备 → `set_keep_alive(true)`；配对成功增、解除配对减（并断开该连接）
- [x] 3.8 状态推导切换：`DeviceManager::get_devices` 对 Paired 设备改读 presence 状态（Online = Connected|Probing，Offline = Unreachable），确认 `DeviceStatus` 枚举与事件形状零变化
- [x] 3.9 Supervisor 状态机单测：抖动重连不推离线、宽限超时转离线、解配对清理、重探发现回归

## 4. 桌面仓 host 收编 + 桌面验证

- [x] 4.1 core 启动序列接管 bootstrap + presence 启动；删除 `src-tauri/src/commands/lifecycle.rs` 中一次性「announce → bootstrap → check」spawn
- [x] 4.2 `cargo test` + clippy 全绿；桌面↔桌面实测：配对后闲置 10 分钟常绿、拔网线 ~1 分钟内判离线、恢复后 ≤90s 回归在线
- [x] 4.3 桌面仓提交推 develop，记录 rev 供移动仓 bump

## 5. 移动仓跟进

- [x] 5.1 `packages/swarmdrop-core/rust/mobile-core/Cargo.toml` bump swarmdrop-core git rev；删除 `mobile-core/src/network.rs` 中一次性 spawn（start_node 内 tokio::spawn 块）
- [x] 5.2 `cargo build` + bob build typecheck 确认无 FFI 面变化（无需重生成 bindings）；若有变化则走轻量 bindings 重生成路径
- [ ] 5.3 E2E 实测：iOS sim ↔ Android emu 局域网配对后闲置 10 分钟常绿；杀掉一端 ~1 分钟内另一端判离线，重启后 ≤90s 回归
- [ ] 5.4 观察项记录：移动端前台恢复收敛速度（若体感差，另开 follow-up 加前台 nudge）；E2E 中等 presence 的轮询 workaround 是否可简化
