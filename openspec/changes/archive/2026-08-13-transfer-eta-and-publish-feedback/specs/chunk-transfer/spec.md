# chunk-transfer

## MODIFIED Requirements

### Requirement: Progress tracking with speed and ETA

The system SHALL track transfer progress and emit `transfer-progress` events to the frontend.
Progress events SHALL be throttled to at most one every 200ms. Each progress event SHALL include:
session_id, direction, current file info, total/transferred bytes, speed (bytes/sec), and estimated
time remaining.

速度与剩余时间 SHALL 反映**当下**的传输状况：滑窗内没有新样本时，二者 SHALL NOT 继续报告
一个已不成立的旧值。文件边界那一帧 SHALL 必达，不受节流丢弃。

#### Scenario: Progress event throttling
- **WHEN** multiple chunks complete within a 200ms window
- **THEN** the system SHALL emit at most one `transfer-progress` event for that window, reflecting the latest state

#### Scenario: Speed calculation with sliding window
- **WHEN** calculating transfer speed
- **THEN** the system SHALL use a 3-second sliding window of (timestamp, cumulative_bytes) samples to compute average speed in bytes/sec

#### Scenario: 传输停滞超过滑窗长度
- **WHEN** 距最后一个字节样本已超过滑窗长度（3 秒）——例如接收方正在把收齐的文件发布到
  用户目标位置、对端卡住、或本地磁盘 stall
- **THEN** 速度 SHALL 归零，剩余时间 SHALL 为空（而不是继续返回停滞前的旧速率）
- **AND** 前端据此展示占位而非一个冻住的数字

#### Scenario: 单个文件收齐的那一帧
- **WHEN** 某个文件的最后一块落盘，且距上一次发送进度事件不足 200ms
- **THEN** 该帧 SHALL 绕过节流强制发出，使前端能看到该文件的 100%
- **AND** 前端 SHALL NOT 出现「停在 99.x% 后直接跳完成」
