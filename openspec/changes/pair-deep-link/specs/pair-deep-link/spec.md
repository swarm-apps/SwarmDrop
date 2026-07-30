# pair-deep-link

## ADDED Requirements

### Requirement: 桌面与 Android 注册 swarmdrop scheme

桌面三平台（macOS / Windows / Linux）与 Android SHALL 注册 `swarmdrop://` URL scheme，
使落地页的「在 App 中打开」可把邀请 payload 移交给应用。

iOS SHALL NOT 在本期实现（落地页在 iOS 上隐藏或降级该按钮）。

#### Scenario: 已安装应用时点击移交

- **WHEN** 用户在落地页点「在 App 中打开」，且本机已安装 SwarmDrop
- **THEN** 系统唤起应用，邀请 payload 完整送达，进入配对确认流程

#### Scenario: iOS 上的降级

- **WHEN** iOS 用户打开落地页
- **THEN** 页面不展示无效的「在 App 中打开」按钮，改为提示可在浏览器配对或复制链接后在
  App 内粘贴

### Requirement: 深链在冷启动与已运行两种时序都不丢

深链事件 SHALL 在应用冷启动（事件早于前端挂载）与已运行（第二实例 / 系统事件）两种时序下
都被送达前端：未就绪时缓冲，前端挂载时取走；已就绪时直接派发。

缓冲 SHALL NOT 依赖 Tauri 托管状态（冷启动时托管状态可能尚未建立）。

#### Scenario: 冷启动经链接拉起

- **WHEN** 应用未运行，用户点击深链
- **THEN** 应用启动，前端挂载后拿到该邀请，进入配对确认流程，事件不丢失

#### Scenario: 应用已运行时点击链接

- **WHEN** 应用已在运行，用户点击深链
- **THEN** 窗口被唤起并直接收到该邀请

### Requirement: 外部入口在宿主层统一分发

「打开方式」（`file://`）与深链（`swarmdrop://`）SHALL 共用同一套宿主层入口机制
（冷启动缓冲、去抖、跨 FFI 边界的 panic 兜底），按 URL scheme 分流到各自的前端事件。

平台差异 SHALL 封装在分发模块内部，调用方保持无 `cfg` 的统一调用。

#### Scenario: 文件打开行为不变

- **WHEN** 用户经系统「打开方式」用 SwarmDrop 打开一批文件
- **THEN** 行为与改造前一致：路径归一化、短窗口内合并为一次事件、进入快捷发送流程

#### Scenario: 无法识别的 scheme

- **WHEN** 送达的 URL 既不是 `file://` 也不是 `swarmdrop://`
- **THEN** 静默忽略并记录调试日志，不影响其他入口

#### Scenario: 回调内发生 panic

- **WHEN** 分发过程中发生 panic（尤其 macOS 的 ObjC 回调边界）
- **THEN** panic 被兜底降级为错误日志，进程不 abort

### Requirement: 剪贴板感知全局生效且跨路由去重

剪贴板邀请感知 SHALL 在应用主布局层全局生效，不限于配对页。
「已提示过的邀请」状态 SHALL 保存在 store 中，跨路由切换保持。

#### Scenario: 从任意页面进入应用

- **WHEN** 用户复制邀请链接后回到应用，当前停留在传输页或设置页
- **THEN** 仍能感知到剪贴板中的邀请并给出提示

#### Scenario: 同一邀请不重复提示

- **WHEN** 用户忽略一次提示后在应用内切换多个路由并多次触发窗口聚焦
- **THEN** 同一条邀请不再重复亮出提示

### Requirement: 本机自己生成的邀请不触发提示

剪贴板感知 SHALL 在解析验签后比对邀请的 `inviter_id` 与本机 NodeId，相同则静默忽略。

#### Scenario: 复制自己的邀请去分享

- **WHEN** 用户复制本机刚生成的邀请链接，准备发给别人，随后回到应用
- **THEN** 应用不给出任何配对提示

#### Scenario: 收到别人的邀请

- **WHEN** 用户复制别人发来的邀请链接后进入应用
- **THEN** 应用给出非模态提示，提示中包含对端设备名与平台

### Requirement: 感知只亮入口，用户点击才发起

剪贴板感知 SHALL 以非模态形式（顶部条 / toast）提示，SHALL NOT 直接发起配对。
用户点击后 SHALL 展示模态确认卡（设备名 / 平台 / 短指纹），确认后才发起配对。

#### Scenario: 用户忽略提示

- **WHEN** 用户不点击提示条
- **THEN** 不发生任何网络请求或配对动作

#### Scenario: 用户确认配对

- **WHEN** 用户点击提示条并在确认卡上确认
- **THEN** 发起配对流程
