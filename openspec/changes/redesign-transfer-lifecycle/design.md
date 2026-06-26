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

这解决“暂停后旧 chunk/complete 又到达”“恢复后旧 actor 迟到失败事件覆盖新状态”等竞态。

### D4: 恢复协议先探测后提交

恢复流程改为：

```text
ResumeProbe(session_id, local_epoch)
ResumeStateReport(session_id, phase, epoch, manifest, checkpoint, source_fingerprint)
ResumeCommit(session_id, new_epoch, transfer_key, fetch_plan)
ResumeAck(session_id, new_epoch)
```

双方先报告事实，再决定是否能恢复。`cancelled` 不可逆；`fatal_error` 默认不可恢复；`suspended` 下的 paused/interrupted/peer_offline/app_restarted 可恢复。

### D5: 数据面使用 transfer-data 帧协议

数据面基于 `swarm-p2p-core` 的 DataChannel。第一帧必须是：

```text
Hello { session_id, epoch, role, manifest_digest }
```

后续帧包括 `BlockRequest`、`BlockData`、`Ack`、`Abort`、`Finish`。所有帧都必须绑定 session 和 epoch 的上下文，防止跨会话混淆。

### D6: DB 是历史和恢复事实来源

DB 保存 session phase/reason/epoch、文件 manifest、source fingerprint、checkpoint bitmap/ranges、投影字段和可选事件日志。启动清理不再把 receiver interrupted 伪装成 paused，也不把 sender app restart 一律当成不可恢复 failed。

备选方案：继续用内存 active sessions + DB history 双源。拒绝原因是恢复和重启场景需要 DB 成为唯一持久事实来源。

## Risks / Trade-offs

- [Risk] 状态模型 breaking change 较大。→ Mitigation: 以 migration 或开发期清理旧历史切换，避免长期兼容旧语义。
- [Risk] Coordinator 过大。→ Mitigation: 按 event reducer、actor registry、DB repository、frontend projection 分层。
- [Risk] Epoch 集成遗漏会产生隐性竞态。→ Mitigation: 协议类型和 actor event 都强制携带 epoch，并添加旧 epoch 测试。
- [Risk] 数据通道和 checkpoint 并发复杂。→ Mitigation: ReceiverActor 只报告 checkpoint event，最终状态仍由 Coordinator 决定。
- [Risk] 前端需要大幅适配。→ Mitigation: 后端提供稳定投影 DTO，前端只按投影渲染。

## Migration Plan

1. 增加新 DB 字段和实体类型，保留旧表名但替换状态语义。
2. 引入 `TransferCoordinator` 和 repository/projection 层。
3. 将用户命令和网络事件改为进入 Coordinator，而不是直接操作 sender/receiver。
4. 实现恢复控制协议和 epoch 校验。
5. 接入 DataChannel 并实现 transfer-data 帧协议。
6. 重构前端 transfer store 和页面，按后端投影渲染。
7. 移除旧 pause/resume 直接路径。

开发期可选择清空旧 transfer history；正式发布前再决定是否提供旧状态到新状态的 migration。

## Open Questions

- checkpoint 使用 bitmap 继续存储，还是切换为 ranges 以便更适合数据通道帧？
- sender 源文件校验是否在恢复探测时强制重算 checksum，还是先用 fingerprint 快速拒绝再按需重算？
- 是否需要自动恢复：peer 重新 online 后自动 ResumeProbe，还是仅由用户点击恢复触发？
