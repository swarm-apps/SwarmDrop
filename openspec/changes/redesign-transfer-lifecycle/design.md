## Context

当前传输模块已经具备 Offer、分块拉取、加密、进度、DB 历史和断点续传的基础能力，但生命周期语义不够清晰：用户主动暂停、网络异常中断、应用重启、对端取消和真正失败会混入 `paused` / `failed` 等少量状态。

这会导致两个直接问题：第一，恢复入口无法准确判断对端是否仍可恢复；第二，同一次异常后两端可能显示不同状态。新的数据通道能力会把数据面从 request-response 中拆出来，但如果没有统一传输状态机，数据通道只会让并发和竞态更难排查。

## Goals / Non-Goals

**Goals:**
- 建立后端唯一的 `TransferCoordinator`，负责传输生命周期判定、运行时 actor 注册、DB 投影和前端事件。
- 区分“暂停是用户意图”“中断是运行事实”“失败是不可恢复错误”“取消是终止决定”。
- 为每次开始或恢复传输引入 epoch，拒绝旧 epoch 的迟到消息和数据通道。
- 用探测式恢复协议替代当前直接 resume：先互报状态，再提交恢复。
- 使用 `add-p2p-data-channel` 提供的数据通道承载 transfer-data 帧协议。
- 让前端只消费后端状态投影，不再自行拼接活跃传输和历史语义。

**Non-Goals:**
- 不在本 change 中实现 `swarm-p2p-core` 数据通道底层。
- 不改变文件加密算法本身，除非 epoch/key 派生需要调整协议参数。
- 不做跨设备云同步历史。
- 不保留旧 DB 状态语义的长期兼容；开发期允许迁移或清理旧传输历史。

## Decisions

### D1: TransferCoordinator 作为唯一状态机

新增 `TransferCoordinator`，由它接收用户命令、网络事件、actor 事件和启动清理事件，然后写入 DB 并发出前端投影事件。

`SenderActor` / `ReceiverActor` 只负责文件 I/O、加解密、数据帧读写、checkpoint flush。actor 可以因为断网、取消、重启而消失；session 状态不能由 actor 私自决定。

备选方案：继续在 sender/receiver/resume 模块里分散写 DB。拒绝原因是暂停、中断、恢复和完成事件跨越多个模块，分散写入会继续制造状态不一致。

### D2: 状态模型拆成 phase + reason

用 `phase` 表达大状态，用 `suspended_reason` / `terminal_reason` 表达原因：

```text
phase:
  offered | waiting_accept | active | suspended | terminal

suspended_reason:
  local_paused | remote_paused | interrupted | peer_offline | app_restarted

terminal_reason:
  completed | cancelled | rejected | fatal_error
```

UI 文案由投影层映射，不直接读取底层 enum 猜语义。

### D3: Epoch 防止旧消息污染新状态

每次开始传输或恢复传输都生成新的 `epoch`。控制面消息、数据通道 `Hello`、`Complete`、`Abort` 都必须携带 epoch。收到旧 epoch 消息时，Coordinator MUST 忽略或拒绝，不能更新 DB。

这解决”暂停后旧 chunk/complete 又到达””恢复后旧 actor 迟到失败事件覆盖新状态”等竞态。

**epoch 生成与单调性规则：**

- **权威方**：epoch 由本轮发起方生成——首传是 Offer 发起方（sender），恢复是 `ResumeCommit` 发起方。统一规则 `new_epoch = max(local_epoch, peer_reported_epoch) + 1`，`peer_reported_epoch` 取自 `ResumeStateReport`（首传对端无记录，取 0）。
- **单调递增**：epoch 是 per-session 单调计数器，永不回退。
- **持久化防重启回退**：`current_epoch` 与 phase/reason 同事务写入 session 表；重启后从 DB 读取，确保 `app_restarted` 恢复生成的 new_epoch 仍大于任何已下发的旧 epoch，防止重启把旧 actor / 旧数据通道的迟到消息误判为当前 epoch。
- **绑定校验**：Coordinator 用 `(session_id, epoch)` 准入；actor 事件、控制消息、数据通道 `Hello` 中任一 epoch < current 一律丢弃。仅在 `ResumeAck` 之后双方才把 current_epoch 落库为 new_epoch。

### D4: 恢复协议先探测后提交

恢复流程改为：

```text
ResumeProbe(session_id, local_epoch)
ResumeStateReport(session_id, phase, epoch, manifest, checkpoint, source_fingerprint)
ResumeCommit(session_id, new_epoch, transfer_key, fetch_plan)
ResumeAck(session_id, new_epoch)
```

双方先报告事实，再决定是否能恢复。`cancelled` 不可逆；`fatal_error` 默认不可恢复；`suspended` 下的 paused/interrupted/peer_offline/app_restarted 可恢复。

### D5: 数据面使用 transfer-data 帧协议（fetch_plan 驱动的 hybrid 推流）

数据面基于 `swarm-p2p-core` 的 DataChannel（封装 libp2p-stream，单条长生命周期流）。语义采用 **hybrid 推流**：发送方按协商出的 `fetch_plan` 连续推 `BlockData`，接收方稀疏 `Ack` 推进 checkpoint，`BlockRequest` 仅做 gap-fill。这与 iroh-blobs / GraphSync 的"描述一次需求 + 流式推回"一致，既规避逐块 request-response 的 per-block RTT，又保留断点精度。

> 关键认知：吞吐由"在途数据量 + 流控窗口 ≥ BDP"决定，而非 push/pull 语义本身（纯 pull 配深 pipeline 同样能打满链路）。hybrid 推流的价值在于消除每块的应用层请求往返、实现更简单，而非"绕开 RTT 限制"。

第一帧必须是：

```text
Hello { session_id, epoch, role, manifest_digest }
```

`fetch_plan`（本次要传的 range 列表）来自首传的 Offer/Accept 或恢复时的 `ResumeCommit`，在 `Hello` 之后生效。后续帧：

- `BlockData { range, ciphertext }` —— 发送方按 fetch_plan 顺序连续推，不等待逐块请求
- `Ack { checkpoint_offset }` —— 接收方每 N 块或每 T 秒聚合确认一次，驱动 checkpoint flush
- `BlockRequest { range }` —— 仅乱序 / 校验失败 / 未覆盖缺口时补洞，复用同一条流
- `Finish` / `Abort` —— 终止语义

所有帧都必须绑定 session 和 epoch 的上下文，防止跨会话混淆。

### D7: 数据面承载与背压约束

hybrid 推流的吞吐与正确性绑定以下硬约束（依据 libp2p 一手实践）：

1. **QUIC 优先**：跨网主路径走 QUIC，原生 per-stream 流控承担背压，窗口 / 死锁 / silent-drop 问题消失。
2. **TCP/yamux 回退**：确保 `libp2p-yamux ≥ 0.45.1`（rust-yamux v0.13 自适应窗口），不手动覆盖窗口——否则默认 256KB 在 60ms RTT 下吞吐被压到 ~20Mbit/s。
3. **单条长流**：整个传输复用一条数据通道，gap-fill 的 `BlockRequest` 也走同一条流，规避 muxer 开流级 silent-drop。
4. **读写分离**：发送方一个 task 写 `BlockData`、一个 task 读 `Ack`/`BlockRequest`；接收方一个 task 读 `BlockData`、一个 task 写 `Ack`。避免 yamux OnRead 同流双向阻塞死锁。
5. **checkpoint 是恢复唯一事实源**：只有"已落盘且整帧校验通过"的 range 计入 checkpoint；DCUtR 升级 / 断网杀流 → interrupted → 新 epoch 从 checkpoint 续传，损失上界 = 一个 checkpoint 间隔（N 块 / T 秒）。这把"长流随连接迁移而死"从致命问题降级为一次普通的可恢复中断（即 pcp 这类纯 push 工具因缺 checkpoint 而失败的根因，已被本设计提前解决）。

### D6: DB 是历史和恢复事实来源

DB 保存 session phase/reason/epoch、文件 manifest、source fingerprint、checkpoint ranges、投影字段和可选事件日志。启动清理不再把 receiver interrupted 伪装成 paused，也不把 sender app restart 一律当成不可恢复 failed。

备选方案：继续用内存 active sessions + DB history 双源。拒绝原因是恢复和重启场景需要 DB 成为唯一持久事实来源。

## Risks / Trade-offs

- [Risk] 状态模型 breaking change 较大。→ Mitigation: 以 migration 或开发期清理旧历史切换，避免长期兼容旧语义。
- [Risk] Coordinator 过大。→ Mitigation: 按 event reducer、actor registry、DB repository、frontend projection 分层。
- [Risk] Epoch 集成遗漏会产生隐性竞态。→ Mitigation: 协议类型和 actor event 都强制携带 epoch，并添加旧 epoch 测试。
- [Risk] 数据通道和 checkpoint 并发复杂。→ Mitigation: ReceiverActor 只报告 checkpoint event，最终状态仍由 Coordinator 决定。
- [Risk] 前端需要大幅适配。→ Mitigation: 后端提供稳定投影 DTO，前端只按投影渲染。
- [Risk] hybrid 推流在 TCP/yamux 默认窗口下吞吐崩溃、单流随 DCUtR 升级而死、同流读写死锁。→ Mitigation: QUIC 优先 + yamux v0.13 自适应窗口 + 单流复用 + 读写分离 + checkpoint 兜底（见 D7）。

## Migration Plan

实施分两阶段，**Phase A 不依赖 `add-p2p-data-channel`、可与之并行开发**，Phase B 才合流到数据通道：

**Phase A — 生命周期状态机 + 恢复协调（数据搬运暂留现有 req_resp ChunkRequest）：**
1. 增加新 DB 字段和实体类型，保留旧表名但替换状态语义。
2. 引入 `TransferCoordinator` 和 repository/projection 层。
3. 将用户命令和网络事件改为进入 Coordinator，而不是直接操作 sender/receiver；actor 仅做搬运，搬运层暂用现有 req_resp 拉取。
4. 实现恢复控制协议和 epoch 校验。
6. 重构前端 transfer store 和页面，按后端投影渲染。
7. 移除旧 pause/resume 直接路径。

**Phase B — 数据面切换（依赖 `add-p2p-data-channel`）：**
5. 接入 DataChannel，把 actor 搬运层从 req_resp 换成 hybrid 推流 transfer-data 帧协议；Coordinator / epoch / 恢复协议 / DB 投影不变。

> 拆分理由：Coordinator 状态机是最伤筋动骨、最该先稳定的部分，它与 data channel 正交。让 actor 搬运层在 Phase A 暂用现有 req_resp，可使状态层不被 libp2p-stream 封装进度阻塞，并把 data channel 的不确定性（alpha / 窗口 / DCUtR）隔离到最后一跳。Phase A 写的"Coordinator 管 actor 搬运生命周期"本就是 ① 的本职工作，不是一次性浪费——Phase B 只替换搬运实现。

开发期可选择清空旧 transfer history；正式发布前再决定是否提供旧状态到新状态的 migration。

## Resolved / Open Questions

- **checkpoint 表示**（已定）：切换为 **ranges**。hybrid 推流以 range 为单位，`fetch_plan`、`BlockData.range`、gap-fill `BlockRequest.range` 都是 range 粒度，ranges 比 bitmap 更自然，也便于表达任意 chunk 大小。
- **自动恢复**（已定）：第一版仅由用户点击恢复触发 `ResumeProbe`；自动恢复（peer online 自动 probe）会与 epoch / 并发竞态叠加，放后续。
- **源文件校验**（仍开放）：恢复探测时先用 source fingerprint 快速拒绝；是否在兼容时再强制重算整文件 checksum，留待实现期按性能权衡。
