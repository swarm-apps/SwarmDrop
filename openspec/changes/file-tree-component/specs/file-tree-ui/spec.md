## ADDED Requirements

### Requirement: FileTree 组件渲染树形文件列表
`FileTree` 组件 SHALL 接收 `TreeNode[]` 数组和 `mode` 属性，递归渲染文件/文件夹树形列表。组件 SHALL 支持两种模式：`"select"`（选择模式）和 `"transfer"`（传输模式）。

#### Scenario: 渲染选择模式的文件树
- **WHEN** FileTree 接收 mode="select" 和含文件/文件夹的 TreeNode[]
- **THEN** 文件夹节点渲染为 FolderRow（带展开/折叠），文件节点渲染为 FileTreeItem variant="select"，每行右侧显示文件大小和删除按钮

#### Scenario: 渲染传输模式的文件树
- **WHEN** FileTree 接收 mode="transfer" 和含状态信息的 TreeNode[]
- **THEN** 每个文件节点根据其 `status` 字段渲染对应 variant（waiting/transferring/completed/error）

#### Scenario: 空列表
- **WHEN** FileTree 接收空的 TreeNode[]
- **THEN** 组件不渲染任何内容（返回 null）

### Requirement: FileTreeItem 支持 5 种状态变体
`FileTreeItem` 组件 SHALL 根据 `variant` 属性渲染不同的文件行样式。所有变体 SHALL 采用 leftGroup（图标 + 文件名，flex-1）+ rightGroup（辅助信息 + 操作）的两组布局。

#### Scenario: select 变体
- **WHEN** variant="select"
- **THEN** leftGroup 显示蓝色文件图标 + 文件名，rightGroup 显示文件大小 + X 删除按钮

#### Scenario: transferring 变体
- **WHEN** variant="transferring" 且传入 progress 值
- **THEN** leftGroup 显示蓝色文件图标 + 文件名，rightGroup 显示百分比，文件名下方渲染全宽进度条，行背景为 accent 色 + 蓝色边框

#### Scenario: completed 变体
- **WHEN** variant="completed"
- **THEN** leftGroup 显示绿色文件图标 + 文件名，rightGroup 显示文件大小 + 绿色对勾图标

#### Scenario: waiting 变体
- **WHEN** variant="waiting"
- **THEN** leftGroup 显示灰色文件图标 + 灰色文件名，rightGroup 显示灰色文件大小 + 灰色计时器图标

#### Scenario: error 变体
- **WHEN** variant="error"
- **THEN** 行背景为红色浅底（#FEF2F2），leftGroup 显示红色文件图标 + 文件名，rightGroup 显示红色"失败"文字 + 重试按钮

### Requirement: FolderRow 支持展开/折叠
`FolderRow` 组件 SHALL 渲染文件夹行，点击可展开/折叠子节点。展开时子节点区域 SHALL 带有 22px 左缩进和 `border-left` 引导线。

#### Scenario: 折叠状态
- **WHEN** 文件夹处于折叠状态
- **THEN** 显示右箭头 + 文件夹图标 + 文件夹名，rightGroup 显示子项统计 + 操作按钮，子节点不可见

#### Scenario: 展开状态
- **WHEN** 文件夹处于展开状态
- **THEN** 显示下箭头 + 打开文件夹图标 + 文件夹名，行背景为 accent 色，子节点在缩进容器中可见

#### Scenario: 点击切换
- **WHEN** 用户点击文件夹行
- **THEN** 在展开和折叠状态之间切换，带有过渡动画

#### Scenario: 嵌套文件夹
- **WHEN** 文件夹内包含子文件夹
- **THEN** 子文件夹递归渲染，每层增加 22px 缩进和引导线

### Requirement: 文件树排序规则
文件树 SHALL 将文件夹排在文件前面。同类型内 SHALL 按名称字母序排列。

#### Scenario: 混合文件和文件夹
- **WHEN** 同一层级同时包含文件和文件夹
- **THEN** 文件夹显示在前，文件显示在后，各自按名称排序

### Requirement: FileTree 头部显示统计信息
FileTree 组件 SHALL 渲染头部区域，左侧显示标题（如"已选文件"），右侧显示统计信息（如"共 5 项 · 15.2 MB"）。

#### Scenario: 选择模式头部
- **WHEN** mode="select" 且有文件被选择
- **THEN** 头部左侧显示"已选文件"，右侧显示"共 N 项 · X MB"

#### Scenario: 传输模式头部
- **WHEN** mode="transfer"
- **THEN** 头部左侧显示"文件"，右侧显示已完成/总数（如"2/5"）
