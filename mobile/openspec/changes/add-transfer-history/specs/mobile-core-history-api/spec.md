## ADDED Requirements

### Requirement: 通过 uniffi 暴露传输历史查询

mobile-core 的 `MobileCore` 对象 SHALL 通过 uniffi 导出 `list_transfer_history(status_filter: Option<MobileSessionStatus>) -> Vec<MobileTransferHistoryItem>` 方法，返回当前设备本地 SQLite 中存储的全部传输会话记录，可选按状态过滤。

返回数据 SHALL 按 `started_at` 字段降序排列（最新在前），并 SHALL 包含每一笔会话的：会话 ID、方向（send / receive）、对端 ID、对端名、状态、文件列表（含 file_id / name / relative_path / size）、总字节数、已传字节数、错误消息（可空）、保存路径（可空）、开始时间戳（毫秒）、结束时间戳（可空）。

#### Scenario: 拉取全部历史

- **WHEN** RN 端调用 `MobileCore.listTransferHistory(undefined)`
- **THEN** native 端 SHALL 返回 DB 中所有 `transfer_session` 记录映射成的 `MobileTransferHistoryItem` 数组，含 completed / failed / cancelled / paused / 任何残留状态

#### Scenario: 按状态过滤

- **WHEN** RN 端调用 `MobileCore.listTransferHistory(MobileSessionStatus.Completed)`
- **THEN** native 端 SHALL 仅返回 `status === Completed` 的记录

#### Scenario: 历史为空

- **WHEN** DB 中无任何 transfer_session 记录
- **THEN** native 端 SHALL 返回空数组 `[]`，不抛错

### Requirement: 通过 uniffi 暴露单条会话详情查询

mobile-core SHALL 通过 uniffi 导出 `get_transfer_session_detail(session_id: String) -> MobileTransferHistoryItem` 方法。

#### Scenario: 命中的会话

- **WHEN** RN 端用一个 DB 中存在的 session_id 调用
- **THEN** native 端 SHALL 返回对应的 `MobileTransferHistoryItem`

#### Scenario: 不存在的会话

- **WHEN** RN 端用一个不存在的 session_id 调用
- **THEN** native 端 SHALL 抛 `FfiError`，错误码语义为 "session not found"，错误消息可被 RN 端 catch

### Requirement: 通过 uniffi 暴露单条历史删除

mobile-core SHALL 通过 uniffi 导出 `delete_transfer_session(session_id: String) -> void` 方法，从 DB 中物理删除指定会话及其关联的文件记录。

#### Scenario: 删除存在的会话

- **WHEN** RN 端用一个非活跃状态的 session_id 调用
- **THEN** native 端 SHALL 从 DB 中删除该 session 及其 `session_file` 子记录，下次 `listTransferHistory` 不再包含

#### Scenario: 删除不存在的会话

- **WHEN** RN 端用一个不存在的 session_id 调用
- **THEN** native 端 SHALL 不抛错（幂等），或抛可被 RN 安全忽略的 not-found 错误

### Requirement: 通过 uniffi 暴露清空全部历史

mobile-core SHALL 通过 uniffi 导出 `clear_transfer_history() -> void` 方法，物理删除 DB 中所有 `transfer_session` 记录及其文件子记录。

#### Scenario: 清空历史

- **WHEN** RN 端调用 `MobileCore.clearTransferHistory()`
- **THEN** native 端 SHALL 删除所有 transfer_session 与 session_file 记录，下次 `listTransferHistory` 返回空数组

#### Scenario: 清空时存在活跃会话

- **WHEN** 有正在进行中的传输（status = transferring）且 RN 端调用 clear
- **THEN** native 端 SHALL 仍然删除所有 DB 记录，包括活跃会话；UI 层负责在调用前提示用户或拦截

### Requirement: 通过 uniffi 暴露传输恢复

mobile-core SHALL 通过 uniffi 导出 `resume_transfer(session_id: String) -> MobileResumeTransferResult` 方法，根据 DB 中记录的方向（send / receive）分发到共享 `swarmdrop-core` 的 `initiate_resume_as_sender` 或 `initiate_resume`，返回新建会话的元信息。

#### Scenario: 恢复发送端会话

- **WHEN** RN 端对一个 direction=send 且 status=paused / failed 的 session_id 调用 resume
- **THEN** native 端 SHALL 调 `initiate_resume_as_sender` 重新发起对端 offer 协商，并返回 `MobileResumeTransferResult{ direction: "send", session_id, ... }`

#### Scenario: 恢复接收端会话

- **WHEN** RN 端对一个 direction=receive 且 status=paused / failed 的 session_id 调用 resume
- **THEN** native 端 SHALL 调 `initiate_resume` 重新协商，并返回 `MobileResumeTransferResult{ direction: "receive", session_id, ... }`

#### Scenario: 恢复时对端不在线

- **WHEN** resume 调用底层 P2P 链路时对端不可达
- **THEN** native 端 SHALL 抛 `FfiError`，错误消息能够区分"对端离线"等场景，由 RN 端 toast 提示

### Requirement: 启动时自动 reconcile 中间状态

`MobileCore::start_node` SHALL 在 `ensure_db` 完成之后、`runtime::start_node` 之前，对 DB 中所有 `status === Transferring` 的会话执行 `mark_session_failed(session_id, "app_interrupted")`，确保进程崩溃 / 被系统杀死后留下的"半传"状态不会污染活跃列表，也不会被错误地呈现为"正在传输"。`Paused` 是用户主动暂停的合法状态，不参与 reconcile。

#### Scenario: 上次崩溃留下 transferring 状态

- **GIVEN** DB 中存在一条 `status = Transferring` 的旧 session
- **WHEN** 用户重启 app，触发 `start_node`
- **THEN** 该 session SHALL 被标记为 `Failed`，`error_message = "app_interrupted"`，`finished_at = now`

#### Scenario: 正常启动无残留

- **GIVEN** DB 中所有 session 都是终态（completed / failed / cancelled / paused）
- **WHEN** 用户启动 app
- **THEN** reconcile SHALL 不修改任何记录，正常完成 `start_node`

#### Scenario: 大量残留不阻塞启动

- **GIVEN** DB 中有 50 条以上的 stale 会话
- **WHEN** 用户启动 app
- **THEN** reconcile SHALL 在合理时间（< 500ms 本地 SQLite）内完成，不显著延迟节点拉起

### Requirement: uniffi 类型定义

mobile-core SHALL 定义并导出以下 uniffi 类型：

- `MobileTransferHistoryItem` (Record)：见 [Requirement: 通过 uniffi 暴露传输历史查询]
- `MobileTransferHistoryFile` (Record)：file_id (String), name (String), relative_path (String), size (u64)
- `MobileSessionStatus` (Enum)：Transferring / Paused / Completed / Failed / Cancelled（对齐 entity::SessionStatus）
- `MobileResumeTransferResult` (Record)：session_id (String), direction (String), peer_id (String), peer_name (String), files (Vec<MobileTransferResumedFile>), total_size (u64), transferred_bytes (u64)

类型字段 SHALL 与桌面端 Tauri 暴露的 `TransferHistoryItem` / `ResumeTransferResult` 在语义上对齐，但允许在 ABI 形式上有差异（如 Uuid 序列化为 String、DateTime 序列化为 i64 毫秒 epoch）。

#### Scenario: 类型在生成的 TS bindings 中可用

- **WHEN** 跑 `ubrn build`
- **THEN** `packages/swarmdrop-core/src/generated/swarmdrop_mobile_core.ts` SHALL 导出 `MobileTransferHistoryItem`、`MobileTransferHistoryFile`、`MobileSessionStatus`、`MobileResumeTransferResult` 类型与对应 FfiConverter
