## MODIFIED Requirements

### Requirement: 移动端应用内更新通道

移动端（Android）SHALL 通过 SwarmHive 更新引擎完成「检查 → 下载 → 安装」全流程，
全程留在应用内，不跳浏览器。引擎由 `<UpdateProvider>` 在根布局装配一次，
经 `@swarm-hive/sdk` 的 `rnAdapter` 驱动。

iOS SHALL 不渲染任何应用内更新入口 —— 该平台的升级路径是 TestFlight / App Store，
渲染一个必然「检查失败」的分组只是噪音。

#### Scenario: Android 启动后自动检查

- **WHEN** 应用启动，`<UpdateProvider>` 完成装配
- **THEN** 系统按 `currentVersion`（`nativeBuildVersion` 即 versionCode）打 SwarmHive endpoint
- **THEN** 有更新则状态变 `available`（或 `force-required`），无则 `up-to-date`

#### Scenario: 回前台复核

- **WHEN** app 从后台回到前台（AppState → `active`）
- **THEN** 系统再检查一次，走引擎自带的 recheck 节流，不重复打 endpoint

#### Scenario: iOS 不渲染更新入口

- **WHEN** 应用运行在 iOS
- **THEN** 「关于」页不渲染「软件更新」分组，`UpdateHost` 返回 null

### Requirement: 更新状态呈现 SHALL 穷尽全部状态

设置页与关于页的更新状态呈现 SHALL 对 `UpdateStatus` 的全部 8 个取值做穷尽映射，
每个状态一个明确分支，SHALL NOT 用「特判几个 + else 兜底」的二元/三元判据推导。

`ready` SHALL 有独立分支，呈现为可操作的安装入口。它 SHALL NOT 落入「已是最新」分支。

这条约束存在的原因：`hasUpdate = status === "available" || status === "force-required"`
这类判据会让 `ready`、`downloading`、`idle` 一起掉进 else，于是设置页显示「✅ 已是最新」
的同时，前面压着一个说有新版本要装的弹窗。

#### Scenario: ready 态的关于页呈现

- **WHEN** `status === "ready"`
- **THEN** 「软件更新」行显示可点击的安装入口，而非「已是最新」
- **AND** 点击后触发 `install()`

#### Scenario: 新增状态不会静默落入错误分支

- **WHEN** 为 `UpdateStatus` 增加一个取值
- **THEN** 穷尽 switch 的 exhaustive 检查在编译期报错，而不是运行时显示错误文案

### Requirement: 下载在后台完成 SHALL 通过通知告知用户

当更新产物就绪（`status` 进入 `ready`）而应用不在前台时，Android SHALL 发一条本地通知
告知「新版本已下载，点击安装」。点击该通知 SHALL 把应用带回前台。

这条通知同时是故障的解法而不只是提醒：用户点击通知触发的 Activity 启动，属于 Android
Background Activity Launch 限制的合法例外，应用回到前台后即可正常拉起系统安装确认框。

通知 SHALL 复用 `src/core/notifier.ts` 既有的告警渠道与**只检查不请求**的权限探测 ——
更新通知不值得为它单独弹一次权限请求框。

系统 SHALL NOT 发送下载进度通知，也 SHALL NOT 为「有新版本可用」发通知：
前者与前台服务的传输进度通知抢同一块常驻区域，后者与应用内弹窗重复劝说。

#### Scenario: 熄屏期间下载完成

- **GIVEN** 用户点击更新后熄屏，下载在后台完成
- **WHEN** `status` 进入 `ready` 且 app 不在前台
- **THEN** 发出一条「新版本已下载，点击安装」的通知

#### Scenario: 点击通知回到应用并安装

- **WHEN** 用户点击该通知
- **THEN** 应用回到前台
- **AND** 自动尝试一次安装，系统安装确认框弹出

#### Scenario: 应用在前台时不发通知

- **GIVEN** 下载完成时 app 正在前台
- **WHEN** `status` 进入 `ready`
- **THEN** 不发通知（应用内 UI 已经在承载这件事）

#### Scenario: 未授予通知权限时静默降级

- **GIVEN** 用户从未授予通知权限
- **WHEN** 产物在后台就绪
- **THEN** 不弹权限请求框、不发通知，应用内 ready 态照常可用

### Requirement: 弹层内的可滚动区域 SHALL 可滚动

弹窗类组件的内容容器 SHALL NOT 通过恒真的 `onStartShouldSetResponder` 抢占触摸响应者，
否则 Android 上会 `blockNativeResponder`，把子层原生 `ScrollView` 的滚动整个禁掉。

`@rn-primitives/dialog` 的 `Content` 默认就是这么做的（本意是挡住点击穿透到遮罩，
但那层防御在 native 上本就是空的：`Overlay` 走 `asChild`，`onPress` 被转发给
`Animated.View`，而 View 不支持 `onPress`），因此本仓的 `components/ui/dialog.tsx`
SHALL 覆写这一行为。

`@rn-primitives/alert-dialog` 的 `Content` 是纯 `View`、不抢响应者，**无需覆写**——
所以走 AlertDialog 的强制更新弹窗与进度弹窗一直是能滚的，卡住的只有走 Dialog 的
提示弹窗。`popover` / `select` / `tooltip` 与 `dialog` 同款，将来在它们里面放可滚区域
会踩同一个坑。

#### Scenario: 更新弹窗的 release notes 可滚动

- **GIVEN** release notes 内容高度超过 `ReleaseNotesView` 的 `maxHeight`
- **WHEN** 用户在该区域上下滑动
- **THEN** 内容跟随滚动，能看到被截断的尾部

#### Scenario: 点击弹窗内容不会误关弹窗

- **WHEN** 用户点击弹窗内容区域（非遮罩）
- **THEN** 弹窗保持打开

### Requirement: 已下载**完成**的产物 SHALL 跨进程存活

移动端 SHALL 不因应用退出而丢弃**已完成并通过校验**的更新产物：它经 SDK 的 `reconcile`
端口在下次检查时恢复为 `ready`，跳过整个下载阶段。

**未完成的下载不在此列** —— 中断后重开是全量重下。expo 的 `resumeData` 只有 `pauseAsync()`
能产出，而进程被杀时没有任何钩子能调到它（推导见上游 change 的 design D7）。UI 与文档
SHALL NOT 暗示中断的下载会被接续。

具体行为由上游 `@swarm-hive/sdk` 与 `@swarmhive-rn` registry 提供（SwarmHive 仓的
`ready-state-durability` change）；本仓的义务是**不就地修改**这些由 registry 分发的文件，
并在升级 SDK 时同步拉取 registry。

#### Scenario: 下载完成后杀进程重开

- **GIVEN** 某版本的 APK 已下载完成并通过校验
- **WHEN** 用户杀掉应用后重新打开，检查更新
- **THEN** 状态直接进入 `ready`，不重新下载

#### Scenario: 下载中途杀进程重开

- **GIVEN** 下载进行到约 60% 时进程被杀
- **WHEN** 用户重新打开应用并再次下载同一版本
- **THEN** 残留文件被清掉，从 0 重新下载（这是已知取舍，不是缺陷）
