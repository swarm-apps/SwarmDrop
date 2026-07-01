## Context

当前 `FileList` 组件（`src/components/transfer/file-list.tsx`）是一个扁平列表，接收 `SelectedFile[]` 数组逐行渲染，无文件夹递归展示能力。设计稿中已完成 7 个可复用文件树组件（5 种文件状态 + 2 种文件夹状态），采用 leftGroup + rightGroup 两组布局，支持缩进引导线。

现有类型基础：
- `TransferFileInfo`（`src/commands/transfer.ts`）：含 `fileId`、`relativePath`、`size`、`isDirectory`
- `SelectedFile`（`src/components/transfer/file-list.tsx`）：含 `path`、`name`、`size`、`isDirectory`
- `TransferProgressEvent`：含 `currentFile.fileId`

## Goals / Non-Goals

**Goals:**
- 基于 `@headless-tree/react` 实现树形文件列表组件，支持文件夹展开/折叠
- 内置搜索/过滤能力（searchFeature）
- 支持虚拟滚动（@tanstack/react-virtual），应对大文件夹场景
- 组件支持两种模式：选择模式（select）和传输模式（progress/complete/waiting/error）
- 提供 `useFileSelection` Hook，管理文件选择状态（entry points + 去重 + relativePath 计算）
- 适配桌面端和移动端（组件本身响应式，宽度由父容器决定）

**Non-Goals:**
- 不实现拖拽排序（headless-tree 支持，但文件传输场景不需要）
- 不实现多选（headless-tree 支持，但文件选择用自己的 Map 管理）
- 不实现文件重命名
- 不处理后端 Rust 文件枚举逻辑（由 `prepare_send` 命令完成）

## Decisions

### 1. 树引擎：@headless-tree/react

**选择**: 使用 `@headless-tree/core` + `@headless-tree/react` 管理树的展开/折叠、搜索、键盘导航等行为。

**理由**:
- Headless 架构：只管状态和行为，UI 完全自定义，与我们的设计稿完美匹配
- 内置 feature 系统：搜索（searchFeature）、键盘导航（hotkeysCoreFeature）按需引入
- 扁平渲染：`tree.getItems()` 返回已展开的可见节点扁平列表，天然适配虚拟滚动
- 轻量：core 9.5kB + react 0.4kB，tree-shaking 友好
- 数据适配简单：通过 `dataLoader.getItem()` / `dataLoader.getChildren()` 接入，与我们的 Map 存储兼容

**替代方案**:
- 自建递归组件 + Radix Collapsible → 需手写展开/折叠、键盘导航、搜索逻辑，~500 行代码
- react-arborist → 带自己的 UI 样式，定制困难
- react-complex-tree → headless-tree 是其官方继任者

### 2. 数据模型：扁平存储 + dataLoader 适配

**选择**: 核心存储为扁平的 `Map<absolutePath, FileMeta>`，通过 `dataLoader` 适配层接入 headless-tree。

**数据流**:
```
Map<absolutePath, FileMeta>
  → buildTreeData()                    // 构建 parentId → childIds 映射
  → dataLoader.getItem(id) / getChildren(id)  // headless-tree 消费
  → tree.getItems()                    // 返回扁平可见节点
  → .map(item => <FileRow />)          // 渲染
```

**dataLoader 实现**:
```typescript
// 每个节点的 ID = relativePath（目录以 / 结尾）
// 虚拟根节点 ID = "root"
const dataLoader = {
  getItem: (itemId: string) => nodeMap.get(itemId),
  getChildren: (itemId: string) => childrenMap.get(itemId) ?? [],
};
```

**理由**: 与传输协议对齐（manifest 是扁平的 FileEntry[]），增删操作 O(1)。

### 3. 虚拟滚动：@tanstack/react-virtual

**选择**: 使用 `@tanstack/react-virtual` 的 `useVirtualizer` 进行虚拟滚动。

**理由**:
- 项目已使用 TanStack 生态（TanStack Router）
- headless-tree 的扁平列表输出天然适配虚拟化
- 仅渲染可视区域节点，支持 10w+ 文件不卡顿

**集成方式**:
```typescript
const virtualizer = useVirtualizer({
  count: tree.getItems().length,
  getScrollElement: () => scrollRef.current,
  estimateSize: () => 32, // 行高 ~32px
});

// 只渲染可见的虚拟项
virtualizer.getVirtualItems().map((virtualItem) => {
  const item = tree.getItems()[virtualItem.index];
  // 渲染 FileTreeItem 或 FolderRow
});
```

### 4. 文件选择管理：useFileSelection Hook

**选择**: 独立 Hook 管理选择状态，不放入 Zustand store。

**数据结构**:
```typescript
interface EntryPoint {
  path: string           // 用户选中的路径
  type: 'file' | 'folder'
}

interface FileMeta {
  absolutePath: string
  name: string
  size: number
  isDirectory: boolean
}

// Hook 内部维护
entries: Map<string, FileMeta>     // absolutePath → meta
entryPoints: EntryPoint[]

// Hook 返回 headless-tree 所需的 dataLoader + 派生数据
interface FileSelection {
  dataLoader: { getItem, getChildren }  // 直接传给 useTree
  rootChildren: string[]                 // 顶层节点 ID 列表
  totalSize: number
  totalCount: number
  addPaths: (paths: string[]) => Promise<void>
  removePath: (absolutePath: string) => void
  clear: () => void
}
```

### 5. 组件拆分策略

```
FileTree (容器，管理 useTree + useVirtualizer)
├── FileTreeHeader  (标题 + 统计 + 搜索框)
└── tree.getItems().map() (扁平渲染，无递归)
    ├── FolderRow   (文件夹行：chevron + 图标 + 名称 | 统计 + 操作)
    └── FileTreeItem (文件行，5 种 variant)
```

- `FileTree`: 唯一对外 API，内部管理 `useTree` 实例和虚拟化
- 无递归渲染 — headless-tree 将树展开为扁平列表
- 缩进通过 `item.getItemMeta().level * 22` 的 `paddingLeft` 实现
- 引导线通过 CSS `border-left` 在每个缩进层级绘制

### 6. 传输状态映射

```typescript
type FileStatus = 'select' | 'waiting' | 'transferring' | 'completed' | 'error'

function getFileStatus(fileId: number, progress: TransferProgressEvent): FileStatus {
  if (progress.currentFile?.fileId === fileId) return 'transferring'
  // completedFiles 通过 fileId 列表判断
  // 默认 waiting
}
```

## Risks / Trade-offs

- **[依赖] 新增 @headless-tree/core + @headless-tree/react** → 体积小（~10kB），API 稳定（v1.2.1），是 react-complex-tree 的官方继任者
- **[适配] 引导线在扁平列表中绘制较复杂** → 需要根据当前节点和下一节点的 level 关系，用 CSS 绝对定位或 border 模拟。可参考 VS Code 文件树的实现方式
- **[UX] Tauri 文件选择器不返回文件 size** → `addPaths` 需调 Tauri 命令获取文件元信息
- **[兼容] 文件夹选择只返回文件夹路径** → `addPaths` 需递归枚举子文件
