## Why

当前传输实现把“用户暂停”“网络中断”“失败”“取消”混在 `paused` / `failed` 等少量状态中，导致中断后恢复入口不稳定，并且两端设备可能显示不一致的传输状态。

在引入通用 P2P 数据通道后，SwarmDrop 需要重建文件传输生命周期：用明确的持久化状态机、恢复协商协议和 epoch 机制，让暂停、中断、恢复、取消成为可解释、可测试的流程。

## What Changes

- 新增 `TransferCoordinator`，作为传输状态、运行时 actor、DB 投影和前端事件的唯一协调者。
- 重建传输状态模型，区分 active、suspended、terminal，以及 `local_paused`、`remote_paused`、`interrupted`、`peer_offline`、`fatal_error`、`cancelled` 等原因。
- 新增传输 epoch，每次开始或恢复传输生成新 epoch；所有控制消息和数据通道握手都必须携带 epoch。
- 用 `ResumeProbe` → `ResumeStateReport` → `ResumeCommit` → `ResumeAck` 替代当前双入口 resume 协议。
- 将控制面保留在 request-response，将文件数据面迁移到 `add-p2p-data-channel` 提供的数据通道。
- 新增 transfer-data 帧协议，用于在数据通道上交换 `Hello`、`BlockRequest`、`BlockData`、`Ack`、`Abort`、`Finish` 等帧。
- 重构数据库模型和启动清理逻辑，使“异常中断”进入可恢复 suspended 状态，而不是伪装成 paused 或 failed。
- 重构前端 transfer store，使前端只渲染后端投影，不自行猜测活跃/历史状态。
- **BREAKING**：替换现有 `pause_transfer` / `resume_transfer` / 传输事件 payload / DB status 语义；旧的传输历史可选择迁移或清空。

## Capabilities

### New Capabilities
- `transfer-lifecycle-state`: 传输生命周期状态机、持久化投影和前端状态语义。
- `transfer-resume-coordination`: 恢复探测、状态报告、恢复提交和 epoch 防竞态协议。
- `transfer-data-plane`: 基于通用 P2P 数据通道的文件数据面帧协议。

### Modified Capabilities
- None.

## Impact

- 影响 crates：`crates/core`、`crates/entity`、`crates/migration`、`src-tauri`、`src`。
- 影响模块：transfer manager/sender/receiver/resume/progress、protocol、database ops、Tauri commands/events、Zustand transfer store、传输列表和详情页。
- 依赖关系：仅**数据面阶段（Phase B，transfer-data-plane）依赖 `add-p2p-data-channel`**；生命周期状态机与恢复协调（Phase A）不依赖，可与 `add-p2p-data-channel` 并行开发，详见 design.md Migration Plan。
- 数据影响：现有 transfer session 表结构和历史状态语义会发生 breaking change，需要 migration 或开发期清理策略。
- 跨仓影响（SwarmDrop-RN）：本 change 改的 `crates/core`（transfer / protocol / manager→Coordinator）、`crates/entity`（SessionStatus → phase+reason）、`crates/migration` 都是双端共享核心，对移动端 **BREAKING**。需同步改 `mobile-core/src/transfer.rs` 的 uniffi 镜像 Record/enum 与桥接函数签名、RN 的 `src/stores/transfer-store.ts` 投影，并重新生成 ubrn binding。按既定策略：桌面端先落地并稳定，移动端随后在 SwarmDrop-RN 仓单独适配，不在本 change 范围内。
