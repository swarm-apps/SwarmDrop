# transfer-persistence-port Specification

## Purpose
TBD - created by archiving change transfer-store-port-completion. Update Purpose after archive.
## Requirements
### Requirement: 传输持久化的全部读写经端口表达

`swarmdrop-transfer` 的持久化端口 SHALL 覆盖传输域**全部**持久化用例，包括运行时写路径
（会话/文件创建、checkpoint、状态转换、outboard）与历史管理（列表查询、删除单条、清空历史、
源文件路径、过期挂起接收会话回收、会话 origin 标记）。

任何宿主（桌面 / 移动 / Web）SHALL NOT 为传输相关的读写另行持有 ORM 连接或具体存储类型。

端口方法签名 SHALL 只使用纯 DTO（`Uuid` / `Vec<String>` / `TransferProjection` /
`ExpiredReceiverActor` / `entity` 的 scalar `Model`），SHALL NOT 出现携带 ORM 关系类型的
`ModelEx`。

#### Scenario: 宿主需要传输历史

- **WHEN** 任一端的命令层 / 桥接层需要查询或修改传输历史
- **THEN** 它经端口方法完成，不直接构造 ORM 查询，也不为此持有数据库连接

#### Scenario: 端口签名不引入 ORM 类型

- **WHEN** 在端口上新增或修改方法签名
- **THEN** 签名中不出现 sea-orm 的关系类型；`crates/transfer` 在 `wasm32-unknown-unknown`
  target 下仍能编译通过

### Requirement: 端口有出口

`TransferManager` SHALL 提供 accessor，让注入方能取回自己注入的持久化端口实例。

带生命周期语义的操作（如删除会话）SHALL 由 `TransferManager` 的域方法承载，
而不是让每个宿主各自在端口之上重写一遍判定。

#### Scenario: 宿主取回注入的 store

- **WHEN** 宿主在组装点注入了一个 store 实现，随后需要用它做历史查询
- **THEN** 宿主经 `TransferManager` 的 accessor 取回该实例，无需保留第二份引用

#### Scenario: 删除走域方法

- **WHEN** 任一端发起「删除一条传输记录」
- **THEN** 请求经 `TransferManager` 的域方法，由它统一判定是否允许，再委托端口执行

### Requirement: 投影列表顺序确定

列出全部传输投影的端口方法 SHALL 按会话 `started_at` 倒序返回。

各端前端 MAY 按自身维度（收件箱按结束时间、活动视图按更新时间）重排；
端口保证的是**确定性**，不是最终展示顺序。

#### Scenario: 同一份数据两次查询顺序一致

- **WHEN** 在数据未变更的情况下连续两次列出投影
- **THEN** 两次返回的顺序完全相同，且均为 `started_at` 倒序

#### Scenario: 只取最近 N 条

- **WHEN** 消费方对列表结果做 `take(N)` 以展示「最近 N 条」
- **THEN** 取到的是最新开始的 N 条，而非任意 N 条

### Requirement: 删除传输记录只删记录

删除一条传输会话 SHALL 删除该会话行与其文件行，SHALL NOT 删除已落盘的文件，
SHALL NOT 删除关联的收件箱条目（关联字段置空，条目保留）。

删除一个 `suspended` 会话 SHALL 一并清除其断点信息，用户界面 SHALL 在确认时说明这一点。

#### Scenario: 删除已完成的接收记录

- **WHEN** 用户删除一条已完成的接收会话记录
- **THEN** 传输列表中该条消失；收件箱中对应条目仍在且可打开/下载；磁盘或 OPFS 上的文件不受影响

#### Scenario: 删除可续传的挂起记录

- **WHEN** 用户删除一条 `suspended` 且 recoverable 的会话
- **THEN** 确认对话说明断点信息将一并清除、无法再续传；确认后记录被删除

### Requirement: 进行中的传输不可删

处于 `offered` / `waiting_accept` / `active` 的会话 SHALL NOT 被删除。
该约束 SHALL 在域层强制，不得只依赖前端隐藏按钮。

#### Scenario: 界面不提供入口

- **WHEN** 一条会话正在进行中
- **THEN** 传输列表不为它渲染删除入口

#### Scenario: 绕过界面直接调用

- **WHEN** 调用方（MCP 客户端 / 陈旧的前端状态 / 直接调用导出接口）对进行中的会话请求删除
- **THEN** 请求被拒绝并返回明确原因（提示先取消该传输），记录未被删除

### Requirement: 清空历史只清终态

清空传输历史 SHALL 只删除处于 `terminal` 的会话及其文件行，SHALL NOT 影响
`offered` / `waiting_accept` / `active` / `suspended` 会话。

#### Scenario: 传输进行中时清空

- **WHEN** 有一条会话正在传输，用户执行「清空记录」
- **THEN** 已结束的记录被清除，进行中的那条仍在列表里且传输不受影响

#### Scenario: 清空需要二次确认

- **WHEN** 用户在传输页触发清空
- **THEN** 先出现二次确认，文案说明只清空已结束的记录、已接收文件不受影响

### Requirement: 过期挂起接收会话回收经端口且三端调用时机一致

过期回收 SHALL 作为端口方法提供，其命中判据 SHALL 为
`phase = Suspended` 且 `recoverable` 且 `direction = Receive` 且 `updated_at` 早于
`now - 保留期`。

三端 SHALL 在启动清理中**先**把遗留 active 会话转为 recoverable suspended，**再**执行回收，
使三端使用同一组判据。

#### Scenario: 上次运行遗留的 active 接收会话已过期

- **WHEN** 一条接收会话在上次运行中处于 active，且最后更新已超过保留期
- **THEN** 启动清理先把它转为 `suspended(app_restarted)`，随后的回收命中它并转为 terminal
  且写明原因

#### Scenario: 保留期内的挂起接收会话不受影响

- **WHEN** 一条 recoverable suspended 的接收会话最后更新在保留期内
- **THEN** 回收不命中它，它仍可发起续传

### Requirement: Web 端可删除与清空传输历史，且刷新后不复活

Web 端 SHALL 提供删除单条与清空历史的能力，语义与桌面一致（见上文删除与清空两条）。
删除 SHALL 同时从内存与浏览器持久化存储中移除，使页面刷新后不再出现。

#### Scenario: 删除后刷新

- **WHEN** 用户在 Web 传输页删除一条记录，随后刷新页面
- **THEN** 该记录不再出现在传输页，也不再出现在任何由历史回补驱动的视图

#### Scenario: 清空后收件箱仍可用

- **WHEN** 用户在 Web 传输页清空历史
- **THEN** 收件箱中已接收的条目仍在，其文件仍可下载

### Requirement: 从历史重新发送在三端可用

「取回某会话源文件路径」SHALL 作为端口方法提供，并在三端都有出口，使「从历史重新发送」
不再是桌面独有能力。

源路径不可用时 SHALL 诚实降级（提示用户重新选择文件），SHALL NOT 伪装成一键重发成功。

#### Scenario: 源文件仍在

- **WHEN** 用户对一条失败的发送记录选择「重新发送」，且源文件路径仍有效
- **THEN** 直接以原文件与原目标设备进入发送流程，无需重新选择文件

#### Scenario: 源路径已失效

- **WHEN** 源路径为空或已不可访问（如移动端的 SAF 授权已失效）
- **THEN** 流程回退到「预选该设备、请用户重新选择文件」，并给出说明

