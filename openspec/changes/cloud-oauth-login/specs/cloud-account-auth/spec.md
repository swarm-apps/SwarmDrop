# Spec Delta

## Purpose

云存储账户的授权连接与生命周期管理：用户以 BYO 凭证将自己在云存储 provider（首期
Google Drive）的账户接入应用，应用持有并维护其 OAuth token 生命周期。它是
issue #132 云存储集成（receive-to-cloud / send-from-cloud）的登录前置层。

## ADDED Requirements

### Requirement: 不内置任何 provider 凭证

系统 MUST NOT 内置或分发任何云存储 provider 的 client_id / client_secret 默认值。
发起连接所用的 client 凭证 SHALL 仅来自用户在连接界面自行填写的输入。凭证未填写
完整时，连接流程 MUST NOT 启动。

#### Scenario: 未填写 client 凭证时不能发起连接

- **WHEN** 用户未填写 client_id / client_secret 即点击「连接」
- **THEN** 界面提示必填项缺失，不打开浏览器、不产生授权会话

#### Scenario: 仓库与二进制中无默认凭证

- **WHEN** 审视仓库源码与发行产物
- **THEN** 不存在任何 provider 的内置 client_id / client_secret 常量或配置默认值

### Requirement: 授权经由系统浏览器与回环回调完成

发起连接时，系统 SHALL 在系统默认浏览器打开 provider 的授权页，并在本机回环地址
（127.0.0.1 的临时端口）上监听回调；MUST NOT 使用内置 webview 承载授权页。授权
请求 SHALL 携带一次性随机 state，回调完成时 MUST 校验 state 与发起会话一致，不一致
则拒绝。

#### Scenario: 浏览器完成授权后连接成功

- **WHEN** 用户在浏览器中同意授权，provider 重定向回回环地址且 state 校验通过
- **THEN** 系统完成 token 交换，该账户进入「已连接」状态，浏览器侧显示完成提示页

#### Scenario: state 不一致的回调被拒绝

- **WHEN** 收到的回调携带的 state 与本机发起的授权会话不匹配
- **THEN** 该回调被拒绝，不进行 token 交换，授权会话保持等待或超时

#### Scenario: 授权会话超时自动清理

- **WHEN** 授权发起后用户长时间未完成（超时阈值到达）
- **THEN** 授权会话被清理，回环监听关闭，账户状态保持未连接

### Requirement: 连接完成后持有离线刷新凭证

连接成功后，系统 SHALL 持有可在 access token 过期后继续换取新 token 的刷新凭证
（offline access）。刷新凭证仍有效时，系统 SHALL 自动刷新；provider 撤销、
到期或远端轮换与本地落盘之间的崩溃导致旧凭证失效时，系统 SHALL 明确要求重新连接。

#### Scenario: 重启后无需重新授权

- **WHEN** 已连接账户后重启应用
- **THEN** 账户仍为已连接状态，后续云操作使用持久化的刷新凭证换取 access token

### Requirement: 刷新凭证轮换时先持久化再使用

对于每次刷新会轮换刷新凭证的 provider，系统 MUST 串行化同一账户的刷新；收到新
refresh token 后，MUST 在原子持久化成功后才发放新 access token 并允许后续刷新。
持久化失败时 SHALL 停止该账户的云操作并呈现保存失败错误，MUST NOT 使用已失效
的旧 refresh token 重试。远端轮换与本地落盘无法组成原子事务；此窗口内进程崩溃
并导致重启后旧凭证被拒时，系统 SHALL 进入「需要重新连接」。

#### Scenario: 轮换后重启不掉登录

- **WHEN** provider 在一次刷新中返回了新的刷新凭证，随后应用重启
- **THEN** 重启后使用落盘的新刷新凭证刷新成功，账户保持已连接

#### Scenario: 轮换持久化失败不吞错

- **WHEN** 新刷新凭证落盘失败
- **THEN** 该账户的云操作停止并报「凭证保存失败」类错误，不静默用旧凭证重试

#### Scenario: 远端轮换后、落盘前进程退出

- **WHEN** provider 已轮换 refresh token，而进程在新值持久化前退出
- **THEN** 重启后若落盘旧值被 provider 拒绝，账户进入「需要重新连接」；不报告仍已连接

### Requirement: 存储 adapter 只领取短期 access token

账户管理器 SHALL 向进程内存储 adapter 提供带到期时间的 access token；临近过期时
由账户管理器执行按账户串行的刷新。存储 adapter MUST NOT 获取 refresh token 或
client_secret，MUST NOT 自行调用刷新端点，也 MUST NOT 委托 OpenDAL 隐式刷新。
access token 与到期时间 MUST NOT 出现在前端 IPC 返回值或事件中。

#### Scenario: 两个云操作同时遇到过期

- **WHEN** 同一账户的两个云操作同时请求有效 access token
- **THEN** 账户管理器至多发起一次刷新，两者取得同一代有效 token

### Requirement: 凭证失效时呈现明确状态并引导重连

刷新失败（凭证被撤销、过期或无效）时，系统 SHALL 将账户状态呈现为需要重新连接，
并保留用户的 client 凭证配置；MUST NOT 静默删除账户配置或反复静默重试。

#### Scenario: provider 侧撤销后引导重连

- **WHEN** 用户在 provider 侧撤销了应用授权，之后系统尝试刷新
- **THEN** 账户状态变为「需要重新连接」并提示用户，client 凭证配置仍在，重新连接只需再走一次浏览器授权

### Requirement: 账户列表与状态查询

系统 SHALL 提供已连接账户的列表查询，每项至少包含 provider 种类、账户显示标识与
凭证健康状态（正常 / 需要重新连接 / 刷新中）；查询结果 MUST NOT 包含任何凭证本体
（client_secret、refresh_token、access_token）。

#### Scenario: 前端拿到的是状态而非凭证

- **WHEN** 前端查询云账户列表
- **THEN** 返回账户标识与状态字段，任何凭证字段不出现在 IPC 返回值中

### Requirement: 断开连接清除本地凭证

断开连接时，系统 SHALL 尽力调用 provider 的撤销端点使已发出的 token 失效，并删除
该账户的全部本地凭证；撤销端点调用失败时仍 SHALL 删除本地凭证，并向用户如实反馈
撤销是否成功。

#### Scenario: 断开后本地凭证不复存在

- **WHEN** 用户对已连接账户点击「断开」
- **THEN** 该账户的本地凭证文件被删除，账户从列表消失，重新使用需重新连接

### Requirement: 同一 provider 可连接多个账户

系统 SHALL 允许同一 provider 下并存多个已连接账户，各自持有独立的凭证与状态；
任一账户的连接、刷新、断开 MUST NOT 影响其他账户。

#### Scenario: 两个 Google 账户并存

- **WHEN** 用户先后以两个不同 Google 账户完成连接
- **THEN** 列表呈现两个账户，断开其中一个不影响另一个的已连接状态与刷新
