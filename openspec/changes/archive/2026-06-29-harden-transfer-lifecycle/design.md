## Context

数据面已切到 data-channel 推送模型(`spawn_send_data_channel` 推 `BlockData`/`Finish`,接收方读帧)。旧拉取式 req-resp 协议(`ChunkRequest`→`Chunk`/`ChunkError`、`Complete`→`Ack`)的唯一发送方是 v0.6.0 已删除的拉取式 `ReceiveSession`,现已无人发送;但变体、sender 应答、`incoming.rs` 路由、`IncomingTransferRuntime` trait 方法仍留存。`ResumeProbe.local_epoch` 在应答侧 `handle_resume_probe_impl` 未被读取。

遗留 `suspended/recoverable` 接收会话由 `cleanup_recoverable_sessions`(active→suspended)产生,但没有上限:用户长期不恢复则会话与 `.part` 无限堆积。

## Goals / Non-Goals

**Goals:**
- 编译器可验证地删净拉取式 req-resp 协议与 `local_epoch`(死字段),每步 `cargo check` + `e2e_transfer` 回归。
- 新增可复用清理原语:超保留期未恢复的 recoverable suspended **接收**会话转 terminal 并尽力清理其 `.part`,桌面与移动端启动清理共用。

**Non-Goals:**
- 不动 data-channel 帧协议的可观测行为(BlockData/Finish/Abort)。
- 不考虑跨版本线路兼容(新旧端不混用)。
- 不回收保留期内的会话(正常断点续传不受影响),不回收发送方会话(无 `.part`)。

## Decisions

1. **删除顺序:先处理方,后删变体**。先删 `handle_chunk_request`/`handle_complete`(sender + `*_impl` + trait + 路由),再删 `protocol.rs` 的变体,让编译器逐个点出残留 match 臂。备选(先删变体)会一次性炸出大量错误,定位更难。
2. **Ack 帧**:接收方 `handle_block_data` 每块回 `TransferDataFrame::Ack`,发送方 reader 不消费 → 移除回发与变体。若后续需要发送方据 Ack 做窗口/节流,再以独立 change 引入,不在本次保留死字段。
3. **过期回收原语放 core `database/ops.rs`**:`reap_expired_suspended_receives(db, retention_secs) -> AppResult<Vec<(SessionId, files)>>`,只做 DB 判定 + 转 terminal(`TerminalReason::FatalError` 或新增 `Expired` reason)。返回被回收会话的文件清单供调用方清 `.part`。纯 ops 不碰文件系统(host-agnostic 约束)。
4. **`.part` 清理在 host 侧**:重启后原 `ReceiveSession`(及其 `created_sinks`)已不存在,需按文件元数据/路径重建 sink id 交 `FileAccess` 清理。桌面 setup 与移动 reconcile 各自用本端 FileAccess 尽力删除,失败仅告警不阻断。
5. **保留期默认 7 天,可配置**:常量 + 可选 runtime 配置项;两端一致。
6. **接入点**:桌面 `src-tauri/setup.rs` 启动清理在 `cleanup_recoverable_sessions` 之后调用;移动 `reconcile_stale_sessions`(已复用 coordinator)之后调用。两端都在 node 启动早期、DB 就绪后执行。

## Risks / Trade-offs

- [协议删除误删 live 路径] → data-channel 完成已确认走 Finish 帧;每步 cargo check + e2e_transfer 全套回归;先删处理方让编译器兜底。
- [`.part` 路径重建清理不准,误删/漏删] → 仅对被判定过期的会话清理;清理失败仅告警;加单测覆盖"超期回收 + 文件存在则删除"。
- [`Expired` 终态语义] → 复用 `FatalError` 最省;若要前端区分"过期回收"与"真失败",新增 `TerminalReason::Expired`(需同步 entity/migration/镜像/bindings)——作为可选项,默认复用 FatalError 以缩小面。

## Migration Plan

1. 协议删除 + local_epoch + Ack:桌面 core 一个 PR,cargo check + e2e 绿后,mobile-core re-pin core rev + 重 build native + 重生成 bindings。
2. 过期回收:core 原语 + 单测 → 两端启动接入 → 手动验证(造一个 8 天前 suspended 接收会话,重启后被回收且 .part 清理)。

## Open Questions

- 保留期是否暴露为用户可配置(设置项)还是固定 7 天?默认固定,后续按需开放。
- 是否需要 `TerminalReason::Expired` 以便 UI 区分?默认复用 FatalError。
