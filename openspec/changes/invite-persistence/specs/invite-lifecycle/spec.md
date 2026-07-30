# invite-lifecycle

## ADDED Requirements

### Requirement: 邀请有效期 24 小时，单一 profile

配对邀请的 TTL SHALL 为 24 小时，所有邀请使用同一套生命周期语义。
SHALL NOT 存在按用途区分的多种 TTL profile。

#### Scenario: 链接分享后延迟打开

- **WHEN** 用户生成邀请、分享链接，对方在数小时后点开
- **THEN** 邀请仍然有效，可完成配对

#### Scenario: 超过 24 小时

- **WHEN** 邀请创建满 24 小时后被使用
- **THEN** 判定为过期，拒绝配对并给出「邀请已过期」的明确原因

### Requirement: 邀请跨重启存活

邀请注册表 SHALL 经存储端口持久化：native 走 SQL 实现、Web 走 IndexedDB 写穿。
应用重启后，未过期且未消费的邀请 SHALL 仍然有效。

#### Scenario: 发起方重启应用

- **WHEN** 用户生成邀请并分享出去，随后重启应用，对方才点开链接
- **THEN** 邀请仍然有效，配对可正常完成

#### Scenario: Web 端刷新页面

- **WHEN** Web 端生成邀请后刷新页面
- **THEN** 邀请仍在有效列表中（IndexedDB 读回），未过期则仍可被消费

### Requirement: 一次性语义不因落盘而破

一次性消费 SHALL 以内存表内的原子检查-置换为权威判定点，落盘为其后置写穿。
两方同时消费同一邀请时 SHALL 恰有一方成功。

落盘失败 SHALL NOT 回滚内存中已置换的状态 —— 状态宁可比库更严格。

#### Scenario: 两台设备同时消费同一邀请

- **WHEN** 两台设备在同一时刻用同一邀请发起配对
- **THEN** 恰有一台成功，另一台收到「邀请不可用」

#### Scenario: 写库失败

- **WHEN** 内存 CAS 已置换为 Consumed，但写库失败
- **THEN** 本次消费仍然成功，内存状态不回滚；该邀请在本次运行期间不可再被消费

### Requirement: 发起方可见并可撤销已发出的邀请

发起方 SHALL 能列出当前未过期、未消费的邀请（创建时间、剩余有效期、状态），
并 SHALL 能主动撤销任一条。撤销 SHALL 立即生效且幂等。

#### Scenario: 撤销一条在外流通的邀请

- **WHEN** 用户在邀请列表里撤销某条邀请，随后对方点开该链接
- **THEN** 配对被拒绝，原因为邀请不可用

#### Scenario: 重复撤销

- **WHEN** 同一条邀请被撤销两次
- **THEN** 第二次为 no-op，不报错

### Requirement: capability 明文永不落盘或进日志

持久化 SHALL 只存 `sha256(capability)` 与元数据（inviter、过期时间、状态、创建时间）。
capability 明文与邀请全串 SHALL NOT 写入数据库、IndexedDB 或任何日志。

#### Scenario: 重启后查看邀请列表

- **WHEN** 应用重启后用户打开邀请列表
- **THEN** 列表显示创建时间、剩余有效期与状态，但**不显示原始邀请链接**（明文未落盘）；
  用户若需再次分享，生成新邀请并撤销旧的

### Requirement: 过期与已消费条目的清理时机

启动加载时 SHALL 清除已过期条目；已消费的条目 SHALL 保留至其过期时间之后才删除。

#### Scenario: 已消费条目被过早删除会破坏一次性

- **WHEN** 一条邀请已被消费，且其 TTL 尚未到期，此时应用重启
- **THEN** 该条目仍在库中并保持 Consumed 状态，同一邀请不会因重启而再次可用
