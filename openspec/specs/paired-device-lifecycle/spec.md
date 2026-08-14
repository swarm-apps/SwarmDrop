# paired-device-lifecycle Specification

## Purpose
TBD - created by archiving change atomic-unpair-and-paired-device-store. Update Purpose after archive.
## Requirements
### Requirement: 已配对设备持久化经独立端口

宿主端口层 SHALL 提供独立的 `PairedDeviceStore` 端口承载已配对设备列表的持久化，
与承载身份密钥材料的 `KeychainProvider` 分离。`KeychainProvider` SHALL NOT 再包含
任何已配对设备相关方法。

`PairedDeviceStore` SHALL 只暴露整份快照的读写（load / save）两个方法。
列表算法——新增或更新一条（upsert）、更新信任策略、移除一条——SHALL 由 `swarmdrop-core`
统一实现，端口实现 SHALL NOT 自带任何业务判断。

三端（桌面 / 移动 / Web）SHALL 各自提供 `PairedDeviceStore` 实现；宿主 MAY 用同一个
具体类型同时实现两个端口（移动端即如此），也 MAY 只实现其中一个（Web 无 keychain）。

#### Scenario: Web 端无需 keychain 也能持久化设备列表

- **WHEN** Web 端（wasm）需要跨刷新保留已配对设备
- **THEN** 它只实现 `PairedDeviceStore`（IndexedDB 后端），不需要提供任何身份密钥方法，
  也不存在「实现了但永远不该被调用」的空方法

#### Scenario: 端口实现不承载列表语义

- **WHEN** 新增一台已配对设备
- **THEN** 端口只收到一份完整列表并整份写下；「已存在条目该保留哪些字段」的判断发生在
  core，三端实现里不出现该逻辑的任何副本

### Requirement: 解除配对是 core 内的单一原子操作

`swarmdrop-core` SHALL 提供一个入口一次完成解除配对的全部副作用：写入持久化、
从共享的已配对设备内存表移除、发布 `CoreEvent::PairedDeviceRemoved`。
宿主 SHALL NOT 再自行拼装其中任意两步。

副作用顺序 SHALL 是 **fail-closed**：先持久化，成功后才动内存表并发事件。
持久化失败时该操作 SHALL 整体返回错误，且内存表 SHALL 保持不变。

该操作 SHALL 幂等：目标设备在内存表与持久化列表中都不存在时，操作成功但
SHALL NOT 发布 `PairedDeviceRemoved`。

`CoreEvent` SHALL 包含 `PairedDeviceRemoved` 变体，携带被移除设备的 `peer_id`。
每个宿主 SHALL 显式消费该事件（桌面转应用层事件、移动转 FFI 事件、Web 至少记录），
SHALL NOT 让它落入事件分发的 catch-all 分支。宿主在消费该事件时 SHALL NOT
重复执行持久化删除。

宿主 SHALL NOT 再在解除命令里手工发布「设备列表已变化」的通知——列表变化由该事件表达。

#### Scenario: 正常解除

- **WHEN** 用户对一台已配对设备发起解除
- **THEN** 持久化列表、内存表中该设备均已移除，且恰好发布一次 `PairedDeviceRemoved`

#### Scenario: 持久化写入失败

- **WHEN** 解除过程中持久化写入失败（keychain 不可访问 / IndexedDB 写入被拒）
- **THEN** 操作返回错误，设备仍在列表中，用户看到失败提示；重启后状态与失败前一致，
  不出现「本次运行已解除、重启后又回来」

#### Scenario: 重复解除同一台设备

- **WHEN** 用户对一台已经解除过的设备再次发起解除（重复点击 / 两处入口同时触发）
- **THEN** 操作成功返回当前列表，不发布第二次 `PairedDeviceRemoved`

#### Scenario: 宿主不再手拼两步

- **WHEN** 审阅桌面与移动的解除命令实现
- **THEN** 两端都只有一次 core 调用；不存在「先删内存再写持久化」或其反序的手工序列，
  两端的副作用顺序一致

#### Scenario: 每个宿主都显式接住移除事件

- **WHEN** 审阅三端的 `CoreEvent` 分发实现
- **THEN** 三端都有一个显式的 `PairedDeviceRemoved` 分支；不存在「该事件被 catch-all
  静默吞掉、界面毫无反应」的宿主

### Requirement: 解除配对撤销 presence 维持

解除配对 SHALL 移除共享已配对设备内存表中的条目——这是 `PresenceSupervisor`
撤销该设备保活与重探的**唯一触发判据**（差集 `presence − paired`）。

仅写入持久化而不移除内存表的实现 SHALL 视为未满足本要求。

presence 撤销 SHALL 在解除后的下一个对账周期内完成（当前 1s tick），
验收 SHALL 按「一个 tick 内」而非「立即」判定。

#### Scenario: 解除后停止保活与重探

- **WHEN** 用户解除一台在线设备
- **THEN** 一个对账周期内该设备的 presence 状态被移除、keep-alive 被关闭、连接被断开，
  此后不再对其发起重探

#### Scenario: 解除一台离线设备

- **WHEN** 用户解除一台处于重探退避中的离线设备
- **THEN** 该设备的重探排期被清除，退避计数一并丢弃，不再出现针对它的拨号

### Requirement: 节点未运行时仍可解除配对

节点未启动（无 `PairingManager`）时，宿主 SHALL 仍能解除配对——此时经 core 的
列表算法直接对 `PairedDeviceStore` 执行移除。

该路径 SHALL 只有持久化一个副作用（内存表此刻并不存在），SHALL NOT 要求宿主补做
任何其他步骤。

#### Scenario: 停止节点后解除

- **WHEN** 用户停止节点后在设备列表里解除一台设备，随后重新启动节点
- **THEN** 该设备不出现在启动后的已配对设备中

### Requirement: 解除配对是单方动作

解除配对 SHALL 是本机的单方动作：SHALL NOT 需要对端同意，SHALL NOT 向对端发送任何通知，
对端保存的配对记录 SHALL NOT 因此自动消失。

解除之后，来自该设备的入站请求 SHALL 走完整的配对流程（用户确认），
SHALL NOT 静默恢复为已配对。

#### Scenario: 对端记录不受影响

- **WHEN** A 解除了与 B 的配对
- **THEN** B 的设备列表中仍然有 A，B 未收到任何通知；A 的列表中不再有 B

#### Scenario: 被解除方发起传输

- **WHEN** B 在被 A 解除后向 A 发起传输
- **THEN** A 以「未配对」拒绝该 offer

#### Scenario: 重新建立关系需要重新配对

- **WHEN** B 在被 A 解除后向 A 发起配对
- **THEN** A 弹出完整的配对确认（与首次配对相同的安全闸），用户确认后才重新成为已配对设备

### Requirement: 已配对设备的写入语义在全仓唯一

新增或更新一条已配对设备记录时，若该 `peer_id` 已存在，系统 SHALL 只更新设备元信息
（`os_info` / `paired_at`），SHALL 保留既有的 `trust_level`、`receive_policy`、
`trust_confirmed`。

该语义 SHALL 只存在一份实现；任何宿主 SHALL NOT 自带「整条替换」或其他变体。

#### Scenario: 对已配对设备再次消费邀请

- **WHEN** 用户把一台设备的信任级别调为非默认值后，与该设备再走一次邀请配对
- **THEN** 信任级别与收件策略保持用户设定的值，不被重置为默认

#### Scenario: identify 刷新设备名

- **WHEN** 对端改名后经 identify 广播新设备名，触发已配对设备回写
- **THEN** 设备名更新，信任级别与收件策略不变

### Requirement: 三端均提供解除配对入口

桌面、移动、Web SHALL 各自在设备列表中提供解除配对入口，且 SHALL 在执行前要求
一次明确确认。三端文案 SHALL 表达相同的后果：解除后需要重新配对才能传输文件。

Web 端 SHALL 在解除成功后立即刷新设备清单，SHALL NOT 依赖轮询周期让用户看到结果。

解除失败时 SHALL 向用户呈现失败，SHALL NOT 静默吞掉错误。

#### Scenario: Web 端解除配对

- **WHEN** Web 用户在设备列表点击「解除配对」并完成二次确认
- **THEN** 该设备立即从列表消失；刷新页面后仍不出现

#### Scenario: 确认前不产生任何副作用

- **WHEN** 用户点开确认后取消
- **THEN** 未发生任何持久化写入、内存表变更或事件发布

#### Scenario: 解除失败对用户可见

- **WHEN** 解除因持久化失败而报错
- **THEN** 界面呈现失败且该设备仍在列表中，用户可以重试

