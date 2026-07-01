# bootstrap-node-settings Specification

## Purpose
TBD - created by archiving change bootstrap-node-settings. Update Purpose after archive.
## Requirements
### Requirement: 自定义引导节点持久化
系统 SHALL 在 `preferences-store` 中维护 `customBootstrapNodes: string[]` 字段，持久化用户添加的自定义引导节点地址（Multiaddr 格式）。

#### Scenario: 添加自定义引导节点
- **WHEN** 用户在设置页输入有效的 Multiaddr 地址并确认添加
- **THEN** 地址被追加到 `customBootstrapNodes` 数组并持久化

#### Scenario: 删除自定义引导节点
- **WHEN** 用户在设置页删除某个自定义引导节点
- **THEN** 地址从 `customBootstrapNodes` 数组移除并持久化

#### Scenario: Multiaddr 格式校验
- **WHEN** 用户输入的地址不包含 `/p2p/` 部分或格式无效
- **THEN** 系统 SHALL 拒绝添加并显示格式错误提示

### Requirement: 后端接受自定义引导节点参数
`start` 命令 SHALL 接受可选的 `customBootstrapNodes: Vec<String>` 参数。`create_node_config` SHALL 将自定义节点与默认 `BOOTSTRAP_NODES` 合并。

#### Scenario: 带自定义节点启动
- **WHEN** 前端调用 `start` 并传入自定义引导节点列表
- **THEN** 后端将自定义节点与默认引导节点合并后创建 NodeConfig

#### Scenario: 无自定义节点启动
- **WHEN** 前端调用 `start` 不传入自定义节点（或传空数组）
- **THEN** 后端仅使用默认 `BOOTSTRAP_NODES`，行为与当前相同

### Requirement: 设置页引导节点管理 UI
设置页 SHALL 新增「引导节点」区域，展示默认节点（只读）和自定义节点（可删除），并提供添加自定义节点的输入框。

#### Scenario: 展示默认引导节点
- **WHEN** 用户打开设置页引导节点区域
- **THEN** 显示默认引导节点列表，每项标记为「默认」且不可删除

#### Scenario: 展示自定义引导节点
- **WHEN** 用户已添加自定义引导节点
- **THEN** 自定义节点显示在默认节点下方，每项带有删除按钮

#### Scenario: 添加自定义引导节点交互
- **WHEN** 用户点击添加按钮
- **THEN** 显示输入框供用户输入 Multiaddr 地址，确认后添加到列表

### Requirement: 修改引导节点后重启节点
当节点正在运行且引导节点列表发生变更时，系统 SHALL 提示用户需要重启节点以生效，并提供重启按钮。

#### Scenario: 节点运行中修改引导节点列表
- **WHEN** 节点处于 running 状态且用户添加或删除了自定义引导节点
- **THEN** 显示「引导节点已变更，需重启节点生效」提示和重启按钮

#### Scenario: 用户点击重启节点
- **WHEN** 用户点击重启节点按钮
- **THEN** 系统依次执行 stopNetwork 和 startNetwork（使用新的引导节点列表）

#### Scenario: 节点未运行时修改引导节点列表
- **WHEN** 节点未运行且用户修改了引导节点列表
- **THEN** 仅保存变更，不显示重启提示（下次启动自动使用新列表）

