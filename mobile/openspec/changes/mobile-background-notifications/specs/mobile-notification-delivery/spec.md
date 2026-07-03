## ADDED Requirements

### Requirement: 告警通知经 notify-kit 前台展示

app 必须(MUST)使用 `react-native-notify-kit` 展示配对请求与传输 offer 的告警通知,并且必须(MUST)移除 `expo-notifications` 以避免两套通知栈冲突。前台收到事件时告警必须(MUST)直接展示(banner + 声音),不依赖 `expo-notifications` 的 handler 抑制机制。

#### Scenario: 前台收到配对请求

- **WHEN** app 前台且节点运行时,`PairingRequestReceived` 事件到达 `event-bus.ts`
- **THEN** 经 notify-kit 展示一条带 banner 与声音的系统通知(与既有 in-app 弹窗并存)

#### Scenario: 前台收到传输 offer

- **WHEN** app 前台时,`TransferOfferReceived` 事件到达 `event-bus.ts`
- **THEN** 经 notify-kit 展示一条带 banner 与声音的系统通知

#### Scenario: 不再残留 expo-notifications 栈

- **WHEN** 变更完成后检查依赖与原生合并产物
- **THEN** `expo-notifications` 已从依赖与 `app.json` 移除,通知只经 notify-kit 一条栈投递

### Requirement: 通知深链跳转到相关界面

点击配对或传输通知必须(MUST)经 notify-kit 的事件投递与已定义的 `swarmdrop` scheme 把用户带到对应的应用内界面,无论 app 正在运行还是被点击冷启动。

#### Scenario: 运行中点击配对通知

- **WHEN** app 已在运行,用户点击一条配对请求通知
- **THEN** app 回到前台并导航到该请求的配对 / 响应界面

#### Scenario: 点击通知冷启动 app

- **WHEN** app 未运行,用户点击一条传输 offer 通知
- **THEN** 启动时读取初始通知事件并导航到对应的传输界面

### Requirement: 主动申请通知权限并提供被拒回退

app 必须(MUST)在 onboarding 或首次进入通知设置时申请 `POST_NOTIFICATIONS` 权限,而非在首个入站事件到达时才惰性申请;当权限被拒时,必须(MUST)提供深链到系统设置的重新开启路径,而不是静默失败。

#### Scenario: onboarding 阶段预热权限

- **WHEN** 用户完成 onboarding
- **THEN** 通知权限已被申请过,入站配对事件不会再触发一个打断当下操作的权限弹窗

#### Scenario: 权限此前已被拒

- **WHEN** 通知权限已被拒,之后发生配对 / 传输事件
- **THEN** 应用内的通知权限状态反映"已拒绝",且设置界面提供可深链到系统通知设置的入口

### Requirement: 配置专用 Android 通知渠道

app 必须(MUST)为配对 / 传输告警创建专用的高优先级 Android 通知渠道,并通过 notify-kit 的 Expo config plugin 配置通知图标与 tint 色,而非依赖库自带的 fallback 渠道。

#### Scenario: Android 启动时创建渠道

- **WHEN** app 在 Android 上启动
- **THEN** 存在一个专用高优先级渠道,配对 / 传输告警被投递到该渠道(支持 heads-up 抬头)

#### Scenario: 品牌化通知图标

- **WHEN** 在 Android 上投递一条配对 / 传输告警
- **THEN** 它使用 app 配置的单色通知图标与 tint 色,而非默认的铃铛 / 白色方块

### Requirement: in-app 前台横幅作为并行通道保留

现有的 in-app 配对横幅 / 弹窗路径(`notification-store` → `pairing-request-host.tsx`)必须(MUST)原样继续工作,作为与系统通知并行的前台通道。

#### Scenario: 前台同时存在两条通道

- **WHEN** 前台时到达一条配对请求
- **THEN** in-app 弹窗与系统通知同时呈现,通过任一通道响应都解决同一个 pending 请求且不产生重复
