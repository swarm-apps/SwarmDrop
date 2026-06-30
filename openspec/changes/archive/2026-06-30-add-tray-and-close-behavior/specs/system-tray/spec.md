## ADDED Requirements

### Requirement: 常驻系统托盘图标

桌面应用 SHALL 在启动时创建一个常驻系统托盘图标（macOS 菜单栏 / Windows 通知区域 / Linux StatusNotifierItem），并在应用整个生命周期内保持存在，作为「窗口已关闭但进程仍在后台」时的可见锚点与可靠退出口。托盘图标 SHALL 由桌面壳（Rust）在 `setup` 阶段创建，且其句柄 SHALL 被长期持有以防被释放导致图标消失。主窗口的全部功能 SHALL NOT 依赖托盘可用——托盘仅为加速入口。

#### Scenario: 启动即出现托盘图标

- **WHEN** 应用启动完成
- **THEN** 系统托盘 SHALL 出现 SwarmDrop 图标，无论主窗口可见与否

#### Scenario: 窗口隐藏后托盘仍在

- **WHEN** 主窗口被最小化到托盘（隐藏）
- **THEN** 托盘图标 SHALL 保持存在，且提供唤回窗口与退出应用的入口

### Requirement: 托盘菜单项

托盘 SHALL 提供如下菜单项，按序排列并用分隔线分组：(1) 一条不可点击的**状态行**（复述当前状态）；(2) `打开 SwarmDrop`（默认项，显示并聚焦主窗口）；(3) `暂停接收` / `恢复接收`（随暂停态动态切换文案，切换全局接收暂停状态）；(4) `打开接收文件夹`（在文件管理器打开默认接收目录）；(5) `设置…`（打开设置）；(6) `退出 SwarmDrop`（位于菜单底部，真正退出进程）。其中依赖前端状态的项（`打开接收文件夹`、`设置…`）SHALL 通过向前端 webview 发事件、由 webview 执行，而非在 Rust 侧读取前端偏好。

#### Scenario: 打开主窗口

- **WHEN** 用户点击托盘菜单的 `打开 SwarmDrop`
- **THEN** 系统 SHALL 显示并聚焦主窗口（若此前为隐藏态）

#### Scenario: 退出应用

- **WHEN** 用户点击托盘菜单的 `退出 SwarmDrop`
- **THEN** 系统 SHALL 真正退出进程（不再驻留托盘），停止 P2P 接收

#### Scenario: 从托盘切换暂停接收

- **WHEN** 用户点击 `暂停接收`
- **THEN** 系统 SHALL 进入暂停接收状态，且该菜单项文案 SHALL 变为 `恢复接收`；再次点击 SHALL 恢复接收并切回 `暂停接收`

#### Scenario: 打开接收文件夹

- **WHEN** 用户点击 `打开接收文件夹`
- **THEN** 系统 SHALL 在文件管理器中打开当前配置的默认接收目录

### Requirement: 托盘状态展现

托盘 SHALL 通过**菜单首行文字**反映运行状态（在线 / 暂停 / 离线），使状态在不依赖 tooltip 的环境（如 GNOME 不显示 tooltip）下仍可感知；状态 SHALL 由桌面壳依据「节点是否启动 + 暂停态」派生并更新，前端不直接操作托盘。托盘图标 SHOULD 额外以**形状区分**的三态独立图标（macOS 用 template 单色自适配深浅色）强化状态——该独立图标资产为后续增量，未就绪时三态共用应用图标、状态由文字层承载。

#### Scenario: 在线态

- **WHEN** P2P 节点已启动且未暂停接收
- **THEN** 托盘菜单首行 SHALL 复述类似「在线 · 可接收文件」的状态文字

#### Scenario: 暂停态

- **WHEN** 接收被暂停
- **THEN** 托盘菜单首行 SHALL 复述「已暂停接收」，且暂停项文案 SHALL 切为「恢复接收」

#### Scenario: 离线态

- **WHEN** P2P 节点未启动或网络不可用
- **THEN** 托盘菜单首行 SHALL 复述「未连接」

### Requirement: 托盘左右键交互

托盘 SHALL 尊重各平台的原生惯例：**右键**在所有平台 SHALL 弹出完整菜单。**左键单击**按平台分化——在 **macOS** SHALL 直接弹出菜单（菜单栏额外项的系统原生行为，与 Tailscale / Dropbox 等一致）；在 **Windows / Linux** SHALL 显示并聚焦主窗口。由于部分 Linux 桌面环境不可靠地投递左键事件，「打开 SwarmDrop」菜单项 SHALL 始终存在，作为左键不可用时唤回窗口的兜底入口。

#### Scenario: 右键弹菜单

- **WHEN** 用户在托盘图标上右键
- **THEN** 系统 SHALL 弹出完整托盘菜单

#### Scenario: macOS 左键弹菜单

- **WHEN** 在 macOS 上用户左键单击托盘（菜单栏）图标
- **THEN** 系统 SHALL 直接弹出托盘菜单（与右键一致），用户可从菜单的「打开 SwarmDrop」唤回窗口

#### Scenario: Windows / Linux 左键开窗

- **WHEN** 在 Windows / Linux 上用户左键单击托盘图标
- **THEN** 系统 SHALL 显示并聚焦主窗口

### Requirement: 单实例与平台兼容

应用 SHALL 保证单实例运行：当已有实例在运行时再次启动 SHALL 唤出已存在的主窗口而非新起进程（避免出现重复托盘图标 / 状态错乱）。Linux 打包 SHALL 声明 AppIndicator 运行依赖（`libayatana-appindicator3-dev` 或等价），以保证托盘可显示。

#### Scenario: 二次启动唤出已有窗口

- **WHEN** 应用已在运行（可能已缩盘到托盘），用户再次启动它
- **THEN** 系统 SHALL 显示并聚焦既有实例的主窗口，且 SHALL NOT 启动第二个进程或第二个托盘图标
