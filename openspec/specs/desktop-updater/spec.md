# desktop-updater Specification

## Purpose
TBD - created by archiving change auto-update. Update Purpose after archive.
## Requirements
### Requirement: 桌面端更新检测
系统 SHALL 在应用启动后延迟 3 秒自动检查一次更新，通过 tauri-plugin-updater 向配置的 endpoints 请求 latest.json。

#### Scenario: 启动时自动检测到新版本
- **WHEN** 应用启动并延迟 3 秒后
- **THEN** 系统自动向 latest.json endpoint 发起请求，比较远程版本与本地版本
- **THEN** 若远程版本高于本地版本，更新状态变为 `available`

#### Scenario: 启动时无新版本
- **WHEN** 应用启动并检查更新后发现远程版本等于本地版本
- **THEN** 更新状态变为 `up-to-date`，不展示任何提示

#### Scenario: 检查更新网络失败
- **WHEN** 请求 latest.json 失败（网络不可用、endpoint 不可达）
- **THEN** 更新状态变为 `error`，静默失败不打扰用户

### Requirement: 桌面端下载更新
系统 SHALL 支持用户在设置页点击更新按钮后开始下载，并展示下载进度。

#### Scenario: 用户触发下载
- **WHEN** 用户在设置页点击「更新到 vX.X.X」按钮
- **THEN** 系统开始下载更新包，状态变为 `downloading`
- **THEN** 实时展示下载进度（百分比、已下载/总大小、速度）

#### Scenario: 下载完成
- **WHEN** 更新包下载完成且签名验证通过
- **THEN** 系统自动安装更新并重启应用

### Requirement: 更新签名验证
系统 SHALL 使用 minisign 对更新包进行签名验证，公钥配置在 tauri.conf.json 中。

#### Scenario: 签名验证失败
- **WHEN** 下载的更新包签名验证不通过
- **THEN** 系统拒绝安装，更新状态变为 `error`，提示用户验证失败

### Requirement: 手动检查更新
系统 SHALL 支持用户在设置页手动触发更新检查。

#### Scenario: 手动检查发现新版本
- **WHEN** 用户在设置页点击「检查更新」按钮
- **THEN** 按钮变为「检查中...」状态
- **THEN** 检查完成后，若有新版本，按钮变为「更新到 vX.X.X」，展示更新信息 banner

#### Scenario: 手动检查无新版本
- **WHEN** 用户点击「检查更新」且已是最新版本
- **THEN** 按钮恢复为「检查更新」，版本描述显示「已是最新版本」

