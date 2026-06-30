# window-close-behavior Specification

## Purpose
TBD - created by archiving change add-tray-and-close-behavior. Update Purpose after archive.
## Requirements
### Requirement: closeBehavior 偏好

应用 SHALL 维护一个关闭行为偏好 `closeBehavior`，取值 `ask`（每次询问）/ `tray`（最小化到托盘）/ `quit`（退出应用），默认 `ask`。该偏好 SHALL 与其它用户偏好一致地持久化（前端 `preferences-store`），并在设置页提供「关闭主窗口时」选项让用户随时改为这三种之一。

#### Scenario: 默认每次询问

- **WHEN** 用户首次安装、尚未选择过关闭行为
- **THEN** `closeBehavior` SHALL 为 `ask`

#### Scenario: 设置页修改

- **WHEN** 用户在设置页将「关闭主窗口时」改为某一选项
- **THEN** 之后点 ✕ SHALL 按新值执行，且该值 SHALL 跨重启保留

### Requirement: 点 ✕ 按 closeBehavior 执行

应用 SHALL 在前端唯一拦截窗口关闭请求（`onCloseRequested`），并按 `closeBehavior` 分支：`tray` SHALL 阻止关闭并隐藏窗口到托盘；`quit` SHALL 放行关闭、真正退出；`ask` SHALL 阻止关闭并弹出首次询问对话框。该拦截 SHALL 同时覆盖三平台的 ✕（macOS 红绿灯关闭、Windows/Linux 自绘关闭按钮、`Alt+F4`），它们本质都触发同一关闭请求。

#### Scenario: tray 模式点 ✕

- **WHEN** `closeBehavior` 为 `tray`，用户点 ✕
- **THEN** 窗口 SHALL 隐藏到托盘，进程 SHALL 继续运行并保持后台接收能力

#### Scenario: quit 模式点 ✕

- **WHEN** `closeBehavior` 为 `quit`，用户点 ✕
- **THEN** 应用 SHALL 真正退出

### Requirement: 首次询问对话框

当 `closeBehavior` 为 `ask` 且用户点 ✕ 时，应用 SHALL 弹出一个二选一对话框：选项为「最小化到托盘」（默认/推荐）与「退出 SwarmDrop」，并说明退出后将离线、不可被发现、收不到文件。对话框 SHALL 含一个「记住我的选择」复选框：勾选并选择后，应用 SHALL 把对应值写入 `closeBehavior`（`tray` 或 `quit`），此后不再弹出；未勾选则保持 `ask`、下次仍询问。文案 SHALL 按平台分化用词（macOS 用「菜单栏」，Windows/Linux 用「托盘」/「通知区域」）。

#### Scenario: 选最小化到托盘且记住

- **WHEN** 用户在对话框选「最小化到托盘」并勾选「记住我的选择」
- **THEN** 窗口 SHALL 隐藏到托盘，`closeBehavior` SHALL 被置为 `tray`，此后点 ✕ SHALL 直接缩盘不再询问

#### Scenario: 选退出但不记住

- **WHEN** 用户在对话框选「退出 SwarmDrop」且未勾选「记住我的选择」
- **THEN** 应用 SHALL 真正退出，且 `closeBehavior` SHALL 仍为 `ask`（下次启动后点 ✕ 仍询问）

### Requirement: 首次缩盘通知

应用首次将窗口最小化到托盘后 SHALL 弹出一次系统通知，告知用户应用仍在后台运行、可接收文件、以及如何真正退出（托盘菜单的「退出」）。该通知 SHALL 仅在首次缩盘时出现一次，不重复打扰。

#### Scenario: 首次缩盘提示

- **WHEN** 用户首次让窗口最小化到托盘
- **THEN** 系统 SHALL 弹出一次说明「仍在后台运行、退出请用托盘菜单」的通知；后续缩盘 SHALL NOT 再次弹出该通知

### Requirement: Cmd+Q 始终真正退出

在 macOS 上，`Cmd+Q` 与应用菜单的退出 SHALL 始终真正退出应用，不受 `closeBehavior` 影响，也不被缩盘逻辑拦截（✕ 与 `Cmd+Q` 语义分离）。在 Windows/Linux（无 `Cmd+Q` 等价物）上，托盘菜单的「退出」SHALL 作为可靠的真退出口。

#### Scenario: macOS Cmd+Q 退出

- **WHEN** 在 macOS 上用户按 `Cmd+Q`（即便 `closeBehavior` 为 `tray`）
- **THEN** 应用 SHALL 真正退出，而非缩盘

