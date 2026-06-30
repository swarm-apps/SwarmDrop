## ADDED Requirements

### Requirement: 设置页关于区域更新展示
系统 SHALL 在设置页「关于」区域展示应用版本和更新状态信息。

#### Scenario: 无更新可用
- **WHEN** 更新状态为 `up-to-date` 或 `idle`
- **THEN** 显示应用图标、名称、当前版本号
- **THEN** 显示「检查更新」按钮（蓝色主色调）
- **THEN** 显示「更新日志」按钮（次要样式）

#### Scenario: 有更新可用
- **WHEN** 更新状态为 `available`
- **THEN** 版本描述变为「版本 vX.X.X · 有新版本可用」
- **THEN** 按钮变为「更新到 vX.X.X」（桌面端）或「前往下载」（移动端）
- **THEN** 展示蓝色更新信息 banner，包含版本号和更新日志摘要

#### Scenario: 下载中（桌面端）
- **WHEN** 更新状态为 `downloading`
- **THEN** 版本描述变为「版本 vX.X.X · 正在更新...」
- **THEN** 按钮变为灰色禁用态「下载中...」
- **THEN** banner 区域替换为进度展示：进度条 + 百分比 + 已下载/总大小 + 速度

### Requirement: 强制更新弹窗
系统 SHALL 在需要强制更新时展示全屏模态弹窗。

#### Scenario: 桌面端强制更新弹窗
- **WHEN** 桌面端需要强制更新
- **THEN** 展示深色遮罩 + 居中白色弹窗
- **THEN** 弹窗包含：红色警告图标、「需要更新」标题、版本信息卡片（当前/最低/最新）、黄色警告条、「立即更新」红色按钮
- **THEN** 无关闭按钮

#### Scenario: 桌面端强制更新下载中
- **WHEN** 在强制更新弹窗点击「立即更新」后
- **THEN** 图标切换为蓝色下载图标
- **THEN** 标题变为「正在更新」
- **THEN** 展示进度条 + 进度详情（正在下载 vX.X.X / 百分比 / 已下载大小 / 速度）
- **THEN** 底部提示「下载完成后将自动安装并重启」

#### Scenario: 移动端强制更新弹窗
- **WHEN** 移动端需要强制更新
- **THEN** 展示深色遮罩 + 圆角弹窗（适配移动端尺寸）
- **THEN** 内容与桌面端一致，但按钮为「前往下载」+ external-link 图标
- **THEN** 无关闭按钮

### Requirement: 更新状态管理
系统 SHALL 通过 Zustand store 管理更新生命周期状态。

#### Scenario: 状态流转 — 普通更新
- **WHEN** 应用启动
- **THEN** 状态从 `idle` → `checking` → `available`（有更新）或 `up-to-date`（无更新）

#### Scenario: 状态流转 — 下载安装
- **WHEN** 用户触发下载
- **THEN** 状态从 `available` → `downloading` → `ready`（下载完成）→ 自动安装重启

#### Scenario: 状态流转 — 强制更新
- **WHEN** 检测到版本低于 min_version
- **THEN** 状态从 `checking` → `force-required`
- **THEN** 弹窗展示后用户点击更新，状态变为 `downloading`

#### Scenario: 错误恢复
- **WHEN** 更新检查或下载过程中发生错误
- **THEN** 状态变为 `error`，保留错误信息
- **THEN** 用户可重新触发检查（手动点击按钮或重启应用）
