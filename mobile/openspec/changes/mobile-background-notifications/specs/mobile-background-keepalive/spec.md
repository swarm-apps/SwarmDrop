## ADDED Requirements

### Requirement: Android 前台服务在后台保持节点存活

在 Android 上,当 P2P 节点运行时,app 必须(MUST)运行一个前台服务(经 `react-native-notify-kit` 的 foreground service API,带常驻通知),使 `tokio`+`libp2p` 节点在 app 退到后台或息屏后仍能继续处理事件、对端仍可寻址到本机。该服务必须(MUST)运行在 app 主进程内,且只承担保活职责——不得(MUST NOT)拥有或重复创建节点。

#### Scenario: 后台时收到配对请求

- **WHEN** 节点运行、app 退到后台(息屏),对端发来一条配对请求
- **THEN** 前台服务保持进程存活,节点收到该事件,并展示一条系统通知

#### Scenario: 服务运行在主进程

- **WHEN** 声明并启动前台服务
- **THEN** 它运行在 app 默认进程(无 `android:process=":remote"`),从而共享正在运行的 `net_manager` 与 `tokio` 运行时,而非另起第二个节点

### Requirement: 前台服务生命周期绑定节点运行态

前台服务必须(MUST)与 `mobile-core-store` 的 `runtimeState` 严格同步启停:当且仅当节点运行时服务启用。停止节点必须(MUST)拆除服务并移除其常驻通知。

#### Scenario: 启动节点带起服务

- **WHEN** 节点切换到 `running`
- **THEN** 前台服务随其常驻通知一起启动

#### Scenario: 停止节点拆除服务

- **WHEN** 用户从节点控制面板停止节点
- **THEN** 前台服务被停止、其常驻通知被移除,不留任何悬挂的保活通知

### Requirement: 声明所需 Android 后台权限与类型化服务

Android manifest 必须(MUST)声明 `FOREGROUND_SERVICE`、一个匹配的类型化前台服务权限,以及 `WAKE_LOCK`,并且该 `<service>` 必须(MUST)带显式 `foregroundServiceType` 且不导出。这些主要由 notify-kit 的 Expo config plugin 注入;仓库若需补充项(如选定的 `foregroundServiceType` 或额外权限),必须(MUST)经 config plugin(复用现有 `with-android-manifest` / `with-app-build-gradle` 模式)注入,以保证 `expo prebuild` 可复现。

#### Scenario: 合并后的 manifest 中权限齐备

- **WHEN** app 完成 prebuild 并检查合并后的 Android manifest
- **THEN** `FOREGROUND_SERVICE`、匹配的类型化权限与 `WAKE_LOCK` 均存在,且 `<service>` 声明了 `foregroundServiceType` 并未导出

### Requirement: 前台服务启动遵守 Android 平台规则

服务必须(MUST)在 Android 要求的窗口内(Android 12+ 为 5 秒)promote 到前台以避免 ANR / 崩溃(notify-kit 负责 `startForeground` 时序),且不得(MUST NOT)从后台上下文冷启动;它只在 app 仍持有合法启动上下文时启动(例如节点启动时或前台切换时)。

#### Scenario: 在截止期内 startForeground

- **WHEN** 前台服务被启动
- **THEN** 它在 5 秒内将自身提升为前台,系统不会抛出 ForegroundServiceDidNotStartInTime 崩溃

#### Scenario: 不从被禁止的后台上下文启动

- **WHEN** app 完全在后台且节点已停止
- **THEN** app 不尝试冷启动前台服务(Android 会拒绝),而只在节点启动时或前台时启动它

### Requirement: iOS 深后台接收明确不支持

在 iOS 上,除前台与刚退台的短窗口外,app 不得(MUST NOT)声称能在后台接收配对 / 传输,且本次不得(MUST NOT)引入推送中继。此边界是有据可查的平台限制:iOS 挂起进程后,`libp2p` relay v2 预约掉线且无 store-and-forward,对端便无法再寻址到本机。

#### Scenario: iOS app 深后台被挂起

- **WHEN** iOS app 被挂起(深后台或被杀)且对端尝试配对
- **THEN** 不投递任何通知,且这是本次变更有据可查、可接受的行为(不崩溃、不产生误导性的"已投递"状态)

#### Scenario: iOS 前台与刚退台仍能通知

- **WHEN** iOS app 处于前台或挂起前的短窗口内,有事件到达
- **THEN** 按 `mobile-notification-delivery` 的规定展示系统通知

### Requirement: 节点生命周期契约与 Rust core 保持不变

本次变更必须(MUST)保持 `autoStart` 默认为 `false`,且不得(MUST NOT)改动 Rust core 或其 FFI 表面。前台服务只是 host 侧(notify-kit + config plugin)的事情。

#### Scenario: autoStart 不变

- **WHEN** app 以默认偏好冷启动
- **THEN** 节点不自动启动,且在节点被显式启动前前台服务不运行

#### Scenario: 无 Rust / FFI 改动

- **WHEN** 本变更被实现
- **THEN** `packages/swarmdrop-core/rust/` 下无任何文件被修改,也不新增任何 FFI 导出
