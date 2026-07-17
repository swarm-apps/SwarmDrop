## ADDED Requirements

### Requirement: Android 展示实时传输进度常驻通知

在 Android 上,传输进行期间 app 必须(MUST)展示一条常驻(ongoing)通知,实时反映进度(文件名 / 百分比 / 速率与进度条),并通过按 id 更新的方式刷新。为避免每次刷新都提醒用户,该通知必须(MUST)使用 `onlyAlertOnce`。传输进行期间该通知必须(MUST)不可被划掉(ongoing)。

#### Scenario: 传输过程中进度实时更新

- **WHEN** 一次发送 / 接收正在进行,`TransferProgress` 事件持续到达
- **THEN** 同一条通知按 id 被更新,进度条与文案随之推进,且不为每次更新重复响铃 / 抬头

#### Scenario: 传输期间通知不可划掉

- **WHEN** 传输进行中,用户在通知栏尝试划掉该进度通知
- **THEN** 该通知保持存在(ongoing),直至传输结束或被取消

### Requirement: 进度通知提供暂停 / 取消操作

Android 传输进度通知必须(MUST)提供 pause / cancel action,点击后必须(MUST)路由回 transfer manager 执行对应操作,其效果与在 app 内暂停 / 取消一致。

#### Scenario: 从通知暂停传输

- **WHEN** 用户点击进度通知上的"暂停"action
- **THEN** 经 notify-kit 的 action 事件路由到 transfer manager,传输被暂停,通知文案反映暂停态

#### Scenario: 从通知取消传输

- **WHEN** 用户点击进度通知上的"取消"action
- **THEN** 传输被取消,进度通知被移除,应用内状态与之一致

### Requirement: 进度通知在传输结束后收敛

进度通知必须(MUST)在传输完成 / 失败 / 取消后被移除或转为终态提示,不得(MUST NOT)遗留一条停滞的进度通知。active transfer 期间该通知由前台服务承载(见 `mobile-background-keepalive`),使传输在 app 退后台时不被系统掐断。

#### Scenario: 传输完成后清理

- **WHEN** 传输完成或失败
- **THEN** 常驻进度通知被移除或替换为一条可划掉的终态提示,不遗留停滞进度条

#### Scenario: 退后台时长传不被掐断

- **WHEN** 传输进行中用户把 app 退到后台
- **THEN** 前台服务保活使传输继续,进度通知继续更新

### Requirement: iOS 传输进度仅应用内展示

在 iOS 上,app 必须(MUST)在应用内展示传输进度,且本次不得(MUST NOT)实现锁屏 / 灵动岛 Live Activities。iOS 系统通知无进度条能力,Live Activities(ActivityKit)属独立立项。

#### Scenario: iOS 传输进度

- **WHEN** iOS 上一次传输正在进行
- **THEN** 进度在 app 内界面展示,不出现系统进度条通知,也不启动 Live Activity(符合本次平台边界)
