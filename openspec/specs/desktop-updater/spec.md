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
- **THEN** 状态变为 `ready`，系统自动尝试安装并重启

#### Scenario: 安装被用户中断

- **WHEN** 安装尝试未能完成（如 Windows 上用户在 UAC 提示中取消）
- **THEN** 系统 SHALL 保持在可再次安装的状态，SHALL NOT 要求重新下载
- **THEN** 设置页与更新弹窗 SHALL 提供一个**可点击**的「立即安装」入口

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

### Requirement: 更新 UI SHALL 无死胡同

桌面端更新流程中的任何 UI 状态 SHALL 至少提供一个用户可操作的出口。

Tauri 的安装路径是同步的、通常一帧内就离开 `ready`，但这不是保证 —— 任何
`install()` 失败都会把流程停在那里。三条约束：

1. `ready` 态的主按钮 SHALL **可点击**并触发安装重试，SHALL NOT 与 `downloading`
   共用同一个 disabled 的「下载中…」按钮。
2. 下载进度弹窗 SHALL 可关闭。关闭 SHALL 只隐藏 UI —— 下载继续、状态不变。
   SHALL NOT 同时 `preventDefault` 掉 `onPointerDownOutside` 与 `onEscapeKeyDown`
   却又不提供任何 footer 操作（那是一个没有出口的模态框）。
3. 设置页的状态呈现 SHALL 对全部 8 个 `UpdateStatus` 取值穷尽映射，`ready`
   SHALL 有独立分支。

强制更新弹窗是唯一允许不可关闭的例外，但 SHALL NOT 豁免第 1 条 —— 一个连按钮
都点不动的强更用户没有任何出路。

#### Scenario: ready 态主按钮可操作

- **WHEN** `status === "ready"`
- **THEN** 设置页与更新弹窗的主按钮为启用态，点击触发 `install()`

#### Scenario: 进度弹窗可关闭且不取消下载

- **GIVEN** 下载进行中，进度弹窗显示
- **WHEN** 用户按 Esc 或点击弹窗外部
- **THEN** 弹窗关闭，下载继续，`status` 不变

#### Scenario: ready 态不显示传输速率

- **WHEN** 状态从 `downloading` 变为 `ready`
- **THEN** 进度呈现停止转圈、移除速度读数，文案改为「更新已就绪」
- **AND** SHALL NOT 继续显示「下载中… 100% · 1.0 MB/s」这类停在最后一帧的读数

