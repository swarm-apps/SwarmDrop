# pause-receiving Specification

## Purpose
TBD - created by archiving change add-tray-and-close-behavior. Update Purpose after archive.
## Requirements
### Requirement: 全局暂停接收开关

共享 core SHALL 支持一个全局「暂停接收」运行时状态，可被切换为暂停 / 恢复。处于暂停时，应用 SHALL 保持 P2P 节点在线、可被发现，配对与其它功能 SHALL NOT 受影响——暂停**仅**作用于「是否接受新的传入文件传输」。该状态 SHALL 为运行时态、SHALL NOT 持久化，应用重启后 SHALL 回到「接收中」。

core 的传输运行时（`TransferRuntime`）SHALL 暴露一个查询暂停态的钩子，其默认实现 SHALL 返回「未暂停」，使未实现该开关的平台（如移动端）行为不变。

#### Scenario: 暂停期间仍在线可发现

- **WHEN** 接收被暂停
- **THEN** P2P 节点 SHALL 保持在线、可被已配对设备发现，配对流程 SHALL NOT 受影响

#### Scenario: 重启回到接收中

- **WHEN** 用户暂停接收后重启应用
- **THEN** 应用 SHALL 处于「接收中」状态（暂停态不持久化）

#### Scenario: 未实现平台行为不变

- **WHEN** 某平台未实现暂停开关（沿用 `TransferRuntime` 默认实现）
- **THEN** 暂停查询 SHALL 恒为「未暂停」，传输 offer 处理 SHALL 与引入本能力前完全一致

### Requirement: 暂停期间婉拒新传输 offer

当接收处于暂停态时，对新到的传输 offer（`TransferRequest::Offer`），系统 SHALL 自动以明确理由「对方已暂停接收」（`OfferRejectReason::ReceivingPaused`）婉拒，且 SHALL NOT 缓存该 offer、SHALL NOT 落盘、SHALL NOT 向用户弹确认。该婉拒 SHALL 与既有的 `NotPaired` / `PolicyRejected` 婉拒采用同一处理范式，并 SHALL 在「未配对」校验之后、接收策略评估之前生效。恢复接收后，新到的 offer SHALL 照常按既有策略处理。

#### Scenario: 暂停时收到 offer 被婉拒

- **WHEN** 接收处于暂停态，一个已配对设备发来传输 offer
- **THEN** 系统 SHALL 回以 `accepted: false` 且理由为 `ReceivingPaused` 的结果，SHALL NOT 缓存或落盘该 offer，也 SHALL NOT 向本机用户弹出接收确认

#### Scenario: 恢复后正常接收

- **WHEN** 用户恢复接收后，同一或其它已配对设备再次发来 offer
- **THEN** 系统 SHALL 按既有接收策略正常处理该 offer（自动保存或弹确认），不再婉拒

### Requirement: 暂停接收的命令暴露

桌面端 SHALL 提供设置与查询暂停接收状态的 Tauri command（`set_receiving_paused(paused: bool)` 设置、`is_receiving_paused()` 查询），供托盘菜单与窗口 UI 共同调用与反映；状态变更 SHALL 可被 UI 与托盘观察到（广播状态变更事件），使二者展示保持一致。

#### Scenario: 命令切换状态

- **WHEN** 通过命令暂停或恢复接收
- **THEN** 系统 SHALL 更新全局暂停态，且托盘菜单文案与（若有）窗口内的暂停指示 SHALL 反映新状态

