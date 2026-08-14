## Why

当前文件列表组件（`FileList`）是扁平的单层列表，无法递归展示文件夹内容，也不支持展开/折叠。Phase 3 文件传输需要一个树形文件列表组件，在发送选择、传输进度、接收预览等多个场景中复用，并能清晰展示文件夹层级关系。

## What Changes

- 新增 `FileTree` 组件：支持递归文件夹展示、展开/折叠、左右分组布局（文件名左对齐，大小/状态/操作右对齐）
- 新增 `FileTreeItem` 子组件：支持 5 种状态（select、progress、complete、waiting、error）
- 新增 `FolderRow` 子组件：支持折叠/展开态，带缩进引导线
- 新增 `buildFileTree` 工具函数：将扁平 `FileEntry[]` 列表构建为 `TreeNode[]` 树结构
- 新增 `useFileSelection` Hook：管理文件选择状态（entry points + 绝对路径去重 + 动态计算 relativePath）
- 重构现有 `send.lazy.tsx` 页面：用 `FileTree` + `useFileSelection` 替换旧的 `FileList` + `useState`
- 废弃现有 `FileList` 组件（`src/components/transfer/file-list.tsx`）

## Capabilities

### New Capabilities
- `file-tree-ui`: 文件树 UI 组件（FileTree、FileTreeItem、FolderRow），支持递归展示、展开/折叠、多种状态渲染
- `file-selection`: 文件选择状态管理（useFileSelection Hook + buildFileTree 工具函数 + TreeNode 类型定义）

### Modified Capabilities

## Impact

- **前端组件**: 新增 `src/components/transfer/file-tree.tsx`、`src/components/transfer/file-tree-item.tsx`、`src/components/transfer/folder-row.tsx`
- **工具函数**: 新增 `src/lib/file-tree.ts`（buildFileTree、TreeNode 类型）
- **Hook**: 新增 `src/hooks/use-file-selection.ts`
- **页面**: 修改 `src/routes/_app/send.lazy.tsx`，替换 FileList 为 FileTree
- **依赖**: 使用 Radix `@radix-ui/react-collapsible` 实现展开/折叠动画（项目已有 Radix 依赖）
- **类型**: 复用现有 `TransferFileInfo`（`src/commands/transfer.ts`），新增 `TreeNode` 类型
