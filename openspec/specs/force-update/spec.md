# force-update Specification

## Purpose
TBD - created by archiving change auto-update. Update Purpose after archive.
## Requirements
### Requirement: 强制更新检测
系统 SHALL 在获取 latest.json 后检查 `min_version` 字段，当本地版本低于 min_version 时触发强制更新。

#### Scenario: 版本低于 min_version
- **WHEN** 应用启动检查更新后发现本地版本低于 `min_version`
- **THEN** 更新状态变为 `force-required`
- **THEN** 系统展示不可关闭的强制更新弹窗

#### Scenario: 版本满足 min_version
- **WHEN** 本地版本大于等于 `min_version`
- **THEN** 按普通更新流程处理（可选更新或已是最新）

### Requirement: 强制更新阻断
系统 SHALL 在强制更新状态下阻止用户使用应用的任何功能，直到完成更新。

#### Scenario: 桌面端强制更新阻断
- **WHEN** 桌面端检测到需要强制更新
- **THEN** 展示全屏模态弹窗，包含版本信息（当前版本、最低要求、最新版本）
- **THEN** 弹窗无关闭按钮、无取消选项
- **THEN** 仅提供「立即更新」按钮，点击后开始下载更新

#### Scenario: 移动端强制更新阻断
- **WHEN** 移动端检测到需要强制更新
- **THEN** 展示全屏模态弹窗，包含相同的版本信息
- **THEN** 弹窗无关闭按钮、无取消选项
- **THEN** 仅提供「前往下载」按钮，点击后跳转浏览器下载 APK

#### Scenario: 强制更新下载进度（桌面端）
- **WHEN** 用户在强制更新弹窗点击「立即更新」
- **THEN** 弹窗切换为下载进度状态，展示进度条、百分比、速度、已下载大小
- **THEN** 下载完成后自动安装并重启

### Requirement: 版本比较规则
系统 SHALL 使用语义化版本（semver）进行版本比较。

#### Scenario: 语义化版本比较
- **WHEN** 比较版本号 "0.8.0" 与 min_version "1.0.0"
- **THEN** 判定本地版本低于最低要求，触发强制更新

#### Scenario: 预发布版本
- **WHEN** 本地版本为 "1.0.0-beta.1"，min_version 为 "1.0.0"
- **THEN** 判定本地版本低于最低要求，触发强制更新

