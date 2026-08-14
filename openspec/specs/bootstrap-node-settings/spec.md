# bootstrap-node-settings Specification

## Purpose
TBD - created by archiving change bootstrap-node-settings. Update Purpose after archive.
## Requirements
### Requirement: 自定义引导节点持久化

系统 SHALL 在**三端各自的偏好存储**中维护用户添加的自定义引导节点地址（Multiaddr 格式）：桌面与移动为 `preferences-store` 的 `customBootstrapNodes: string[]`（落盘），Web 为 localStorage 偏好存储。

Web 端 SHALL 持久化 **custom 与 removed 两个集合**，SHALL NOT 持久化合并后的最终清单——后者会在版本更新更换内置地址时把老用户永久压在旧地址上，故障形态是「升级后突然连不上」且用户无法自查。

持久化清单 SHALL 在节点启动序列中被逐条回放为运行时意图。

#### Scenario: 添加自定义引导节点

- **WHEN** 用户在设置页输入有效的 Multiaddr 地址并确认添加
- **THEN** 地址被追加到自定义清单并持久化

#### Scenario: 删除自定义引导节点

- **WHEN** 用户在设置页删除某个自定义引导节点
- **THEN** 地址从自定义清单移除并持久化

#### Scenario: Web 端刷新后自定义节点仍在

- **WHEN** 用户在 Web 端添加了一条自定义引导节点后刷新页面
- **THEN** 该节点仍在清单中并被重新登记为运行时意图

#### Scenario: Web 端撤销内置节点后刷新不复活

- **WHEN** 用户撤销了某条内置引导节点后刷新页面
- **THEN** 该节点不被重新登记

#### Scenario: 内置清单更新可达老用户

- **WHEN** 新版本更换了内置引导节点地址
- **THEN** 未显式撤销过内置项的用户获得新地址

### Requirement: 后端接受自定义引导节点参数

`start` 命令 SHALL 继续接受可选的引导节点清单参数，其语义 SHALL 为**启动种子**（seed）而非事实源：它用于在节点起来时把 DHT 路由种子接进内核。

启动时的内核登记 SHALL 使用 `InfraRoles { kad_server: true, relay: false }`，SHALL NOT 使用 `InfraRoles::bootstrap()`。中继角色 SHALL 交由 `InfraSupervisor` 按 `public_reachability` 闸门收敛——启动路径无条件以 `relay: true` 登记会绕过该闸门。

该参数 SHALL NOT 被删除：它是 host 配置引导节点唯一无条件的内核登记点，删除后 `public_reachability=false` 的用户将得不到任何 DHT 路由种子，跨网发现整体不可用。

#### Scenario: 带自定义节点启动

- **WHEN** 前端调用 `start` 并传入引导节点清单
- **THEN** 后端将它们登记为 DHT 种子角色并建立候选表条目

#### Scenario: 关闭公网可达性时仍有 DHT 种子

- **WHEN** `public_reachability` 为 false 且节点启动
- **THEN** 内置引导节点仍进入 kad 路由表
- **AND** 不为它们建立 relay reservation

#### Scenario: 无自定义节点启动

- **WHEN** 前端调用 `start` 不传入自定义节点（或传空数组）
- **THEN** 后端仅使用内置引导节点

### Requirement: 设置页引导节点管理 UI

设置页 SHALL 提供「引导节点」区域，展示内置节点与自定义节点，并提供添加入口。

每一条 SHALL 展示其当前状态（来自 `InfraLink`）：连接中 / 已就绪 / 连不上（携带 `last_error`）/ 已停用（携带停用原因）/ 仅 DHT 种子。SHALL NOT 只展示地址而不展示状态。

移除入口 SHALL 仅对 `InfraLink.removable == true` 的条目提供（见 `infra-link-status`）。

#### Scenario: 展示内置引导节点

- **WHEN** 用户打开设置页引导节点区域
- **THEN** 显示内置引导节点列表，每项标记来源并展示其当前状态

#### Scenario: 展示自定义引导节点

- **WHEN** 用户已添加自定义引导节点
- **THEN** 自定义节点带有当前状态与移除入口

#### Scenario: 失败条目展示原因

- **WHEN** 某条引导节点处于失败态
- **THEN** 该行展示内核下发的 `last_error` 原文并提供复制入口

### Requirement: 引导节点增删即时生效

系统 SHALL 提供运行时登记与撤销基础设施意图的 IPC（三端各一条，语义一致），使引导节点的增删无需重启节点。

登记 SHALL 同步返回、幂等；撤销 SHALL 真正清除内核常驻意图。收敛进度 SHALL 经 `InfraLink` 观测。

#### Scenario: 运行中添加引导节点

- **WHEN** 节点运行中，用户添加一条引导节点
- **THEN** 该条立即出现在清单中，状态为「连接中」
- **AND** 全程不重启节点

#### Scenario: 添加不可达地址后给出失败原因

- **WHEN** 用户添加一条不可达的引导节点地址
- **THEN** 15 秒内该条转为失败态并携带原因

#### Scenario: 运行中移除引导节点

- **WHEN** 用户移除一条 `removable` 的引导节点
- **THEN** 内核不再对其重试，该条从清单消失，全程不重启节点

### Requirement: 添加引导节点前的同步校验

系统 SHALL 在提交前对输入地址做零网络成本的同步校验，并 SHALL 覆盖：Multiaddr 可解析、含 `/p2p/` 段且 peer id 合法、**传输协议为本端点实际装配的传输之一**、与既有条目（含内置）不重复。

「本端支持哪些传输」SHALL 来自内核（`Endpoint::supported_transports()`），SHALL NOT 在前端或共享包中另建一份清单——部署配置是地址清单，端点能力是内核事实。

系统 SHALL NOT 通过一次性拨号来判定新增引导节点是否可用：`Endpoint::connect` 会把候选地址永久写入地址簿且在已连接时直接返回既有连接快照（对已连上的节点永远返回成功），并且它测的是直连而非 reservation 链路。

#### Scenario: 浏览器拒绝 TCP 地址

- **WHEN** 用户在 Web 端粘贴一条 `/ip4/.../tcp/...` 地址
- **THEN** 提交被拒绝，提示说明浏览器需要 `/webrtc-direct/` 地址

#### Scenario: 缺少 peer id 段

- **WHEN** 用户提交的地址不含 `/p2p/<节点 ID>`
- **THEN** 提交被拒绝并说明缺失的是什么

#### Scenario: 校验不产生网络请求

- **WHEN** 用户提交任意地址
- **THEN** 校验在无网络往返的情况下完成

