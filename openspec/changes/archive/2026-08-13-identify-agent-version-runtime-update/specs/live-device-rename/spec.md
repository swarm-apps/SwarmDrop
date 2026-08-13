# live-device-rename

## ADDED Requirements

### Requirement: 改名对已连接的对端即时生效

设备改名 SHALL 在**不重启节点、不断开任何连接、不刷新页面**的前提下对已连接的对端生效。

新的 `agent_version` SHALL 在改名后**主动推送**给全部已连接对端，而不是等待 identify 的周期性
交换（当前默认 5 分钟）。

尚未连接的对端 SHALL 在下次连接建立时自然拿到新值；SHALL NOT 为离线对端排队补推。

#### Scenario: 两台已连接设备之间改名

- **WHEN** 设备 A 与设备 B 已配对且保持连接，用户在 A 上把设备名改为「书房 Mac」
- **THEN** B 的设备列表在秒级内显示「书房 Mac」；两端节点均未重启，A ↔ B 的连接与 A 的 relay
  reservation 均未中断

#### Scenario: 对端离线时改名

- **WHEN** 设备 B 离线期间，用户在 A 上改名
- **THEN** B 重新上线并与 A 建立连接后，首次 identify 交换即带新名字

#### Scenario: 主动推送丢失时的兜底

- **WHEN** 主动推送因该连接尚未完成首次 identify 交换而被对端丢弃
- **THEN** 新名字仍会在下一次周期性 identify 交换时到达，最终一致；SHALL NOT 因此产生错误提示、
  重试风暴或连接重建

### Requirement: 改名编排收在 core，宿主不自行拼装

改名 SHALL 由 `crates/core` 提供**单一编排入口**，一次完成：写入设备配置持久化 → 更新内存中的
本机 `OsInfo` → 推送 identify → 发布改名事件。

「节点尚未启动」的情形 SHALL 由同一入口处理（只落盘、不推网络、不报错），SHALL NOT 要求各宿主
自行判断分支。

宿主（桌面 / 移动 / Web）SHALL NOT 各自实现「停节点 + 起节点」或任何等效的重启编排。

持久化写入 SHALL 先于网络广播；持久化失败时 SHALL NOT 广播新名字，也 SHALL NOT 更新内存态。

#### Scenario: 持久化失败

- **WHEN** 设备配置写盘失败
- **THEN** 整个改名操作失败并向调用方返回错误，新名字既不广播也不进入内存态；用户看到的名字
  保持改动前的值

#### Scenario: 首次启动时命名（节点未运行）

- **WHEN** 用户在 onboarding 阶段设定设备名，此时 P2P 节点尚未启动
- **THEN** 名字被持久化，操作成功返回；节点随后启动时以该名字广播；SHALL NOT 报错

#### Scenario: 三端行为一致

- **WHEN** 用户分别在桌面、移动、Web 上执行同一次改名
- **THEN** 三端走同一条 core 编排，成功与失败的语义一致，SHALL NOT 出现「一端吞错、一端抛错」
  这类分叉

### Requirement: 改名同时覆盖 identify 与配对面的设备名

改名 SHALL 同时更新 identify 的 `agent_version` 与配对面使用的本机 `OsInfo`（主动发起的配对请求、
改名后新生成的邀请串）。

改名 SHALL 只改变名字字段：本机 `OsInfo` 的 hostname / 平台 / 架构 / **能力集（capabilities）**
SHALL 原样保留，重算出的 `agent_version` SHALL 仍携带改名前的能力声明。

改名前已发出的邀请串 SHALL NOT 被追改——它是一次性、带 TTL 的签名凭证。

#### Scenario: 改名后发起配对

- **WHEN** 用户改名后向新设备发起配对
- **THEN** 对方在配对确认界面看到的是新名字

#### Scenario: 改名后生成新邀请

- **WHEN** 用户改名后生成一条新的配对邀请
- **THEN** 邀请中的展示名是新名字

#### Scenario: 改名前已发出的邀请

- **WHEN** 用户先生成邀请并发给对方，随后改名，对方再使用该邀请
- **THEN** 该邀请仍展示旧名字；配对完成后经 identify 交换刷新为新名字

#### Scenario: LAN Helper 节点改名

- **WHEN** 一台声明了 LAN Helper 能力的设备改名
- **THEN** 对端仍从新的 `agent_version` 中解析出该能力并继续将其视为 LAN Helper，
  SHALL NOT 因改名而丢失能力声明

### Requirement: 网络内核提供运行时 agent_version 更新能力

网络内核 SHALL 提供运行期更新 `agent_version` 的入口，且该入口 SHALL 经后台 actor 命令执行
（上层不直接持有可变的 swarm / behaviour 状态）。

更新 SHALL 同时作用于**已建立**的连接与此后新建的连接：既有连接的下一次 identify 交换（含主动
推送）必须携带新值，SHALL NOT 出现「部分连接仍报旧值」。

以相同值调用 SHALL 是空操作：不下发、不推送、不产生任何网络流量。

内核层入口 SHALL 使用协议术语（`agent_version`）而非业务术语（设备名）——它不解释该字符串的内容。

#### Scenario: 一个对端有多条连接

- **WHEN** 本机与某对端同时存在多条连接（如 TCP 与 QUIC、relay 与直连），此时更新 `agent_version`
- **THEN** 每一条连接后续报出的都是新值

#### Scenario: 幂等调用

- **WHEN** 以与当前完全相同的值调用更新入口
- **THEN** 不向任何对端推送，对端不产生设备信息刷新事件

#### Scenario: 节点已关停

- **WHEN** 在节点已关停后调用更新入口
- **THEN** 返回「通道已关闭」类错误，SHALL NOT panic，也 SHALL NOT 触发节点重建

### Requirement: 对端的接收与持久化路径保持不变

对端 SHALL 沿用既有的 identify 接收路径处理新名字：解析 `agent_version` → 刷新已配对设备信息
→ 发布设备信息更新事件 → 宿主持久化。

主动推送与周期性交换在接收侧 SHALL NOT 被区别对待。

设备信息未发生实际变化时 SHALL NOT 重复发布更新事件。

#### Scenario: 推送与周期交换同源

- **WHEN** 对端先收到一次主动推送、随后收到一次内容相同的周期性交换
- **THEN** 只发布一次设备信息更新事件

#### Scenario: 名字改回与主机名相同

- **WHEN** 用户把设备名改回与本机主机名完全相同的值
- **THEN** 这仍被视为一次真实变更（`agent_version` 的名字槽位被省略，字符串确实不同），对端
  同样更新到新的展示名

#### Scenario: 清空设备名

- **WHEN** 用户把设备名清空
- **THEN** 对端展示名回落到该设备的主机名

### Requirement: 改名不影响进行中的会话

改名 SHALL NOT 中断进行中的文件传输、SHALL NOT 使 relay reservation 失效、SHALL NOT 触发任何
连接的重建或重连。

#### Scenario: 传输过程中改名

- **WHEN** 一次大文件传输正在进行，用户此时改名
- **THEN** 传输继续，进度不回退、不重传；改名照常在对端生效

#### Scenario: Web 端改名

- **WHEN** Web 端用户改名
- **THEN** 页面不需要刷新，节点不重建，该页持有的 relay reservation 保持有效

### Requirement: 改名结果经事件广播给所有界面

改名成功后 SHALL 发布一条改名事件，负载包含原始名字（可为空）与**已计算好的展示名**
（名字为空时回退到主机名）。

各宿主界面 SHALL 消费该事件刷新显示，SHALL NOT 各自重复实现「名字为空则用主机名」的回退逻辑。

#### Scenario: 非发起界面同步

- **WHEN** 用户在某一处（设置页、或桌面 MCP 工具）改名
- **THEN** 同一应用内其他展示设备名的位置在无需手动刷新的情况下更新
