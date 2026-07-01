## ADDED Requirements

### Requirement: buildFileTree 将扁平列表转为树结构
`buildFileTree` 函数 SHALL 接收扁平的文件条目数组，根据 `relativePath` 字段构建 `TreeNode[]` 树结构。自动创建中间目录节点。

#### Scenario: 含嵌套路径的扁平列表
- **WHEN** 输入 `[{relativePath: "project/src/index.ts"}, {relativePath: "project/README.md"}, {relativePath: "photo.jpg"}]`
- **THEN** 返回 TreeNode[]，包含 project/ 目录（含 src/ 子目录和 README.md）和 photo.jpg，目录在前文件在后

#### Scenario: 空列表
- **WHEN** 输入空数组
- **THEN** 返回空 TreeNode[]

#### Scenario: 纯文件无目录
- **WHEN** 输入所有 relativePath 都不含路径分隔符
- **THEN** 返回扁平的 TreeNode[] 文件列表（无目录节点）

### Requirement: TreeNode 类型定义
`TreeNode` 类型 SHALL 包含以下字段：`name`（显示名）、`type`（'file' | 'directory'）、`path`（完整相对路径）、`size`（文件字节数，仅文件）、`children`（子节点，仅目录）、`fileId`（文件标识符，仅文件）。

#### Scenario: 文件节点
- **WHEN** TreeNode 的 type 为 'file'
- **THEN** 必须有 name、path、size、fileId 字段，children 为 undefined

#### Scenario: 目录节点
- **WHEN** TreeNode 的 type 为 'directory'
- **THEN** 必须有 name、path、children 字段，size 为该目录下所有文件的累计大小，fileId 为 undefined

### Requirement: useFileSelection Hook 管理选择状态
`useFileSelection` Hook SHALL 提供文件选择状态管理，包括：添加路径（支持文件和文件夹）、移除路径、清空、获取派生树。

#### Scenario: 添加文件路径
- **WHEN** 调用 addPaths 传入文件绝对路径数组
- **THEN** 每个路径添加到内部 Map，自动获取文件元信息（name、size），tree 属性更新

#### Scenario: 添加文件夹路径
- **WHEN** 调用 addPaths 传入文件夹路径
- **THEN** 记录为 folder 类型的 EntryPoint，递归枚举子文件添加到 Map，tree 属性更新

#### Scenario: 移除单个文件
- **WHEN** 调用 removePath 传入某文件的绝对路径
- **THEN** 该文件从 Map 中删除，tree 属性更新，totalSize 和 totalCount 重新计算

#### Scenario: 重复添加相同文件
- **WHEN** 用户先选了文件夹 A（含 file.txt），删除了 file.txt，再单独选择 file.txt
- **THEN** file.txt 在树中显示在文件夹 A 内部（因为其绝对路径属于 A 的 EntryPoint），而非显示为独立文件

#### Scenario: 来自不同位置的同名文件
- **WHEN** 用户分别选择了 `/a/readme.md` 和 `/b/readme.md`
- **THEN** 两个文件都在树中显示，relativePath 不同（`readme.md` 各自独立），不会冲突

### Requirement: relativePath 动态计算
`useFileSelection` SHALL 根据 EntryPoint 动态计算每个文件的 relativePath，而非在添加时固定。

#### Scenario: 文件夹内的文件
- **WHEN** 文件的绝对路径匹配某个 folder 类型的 EntryPoint
- **THEN** relativePath = folderName + "/" + 相对于 EntryPoint 的路径

#### Scenario: 独立文件
- **WHEN** 文件的绝对路径不属于任何 folder EntryPoint
- **THEN** relativePath = 文件的 basename
