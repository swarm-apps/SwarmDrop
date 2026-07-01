## 1. 依赖安装

- [ ] 1.1 安装 `@headless-tree/core` 和 `@headless-tree/react`
- [ ] 1.2 安装 `@tanstack/react-virtual`

## 2. 类型定义与数据适配

- [ ] 2.1 在 `src/lib/file-tree.ts` 中定义 `TreeNodeData` 类型（name、type、path、size、fileId）和 `FileStatus` 类型
- [ ] 2.2 在 `src/lib/file-tree.ts` 中实现 `buildTreeData(entries)`：从扁平 FileMeta Map 构建 `nodeMap`（id → data）和 `childrenMap`（id → childIds[]），供 headless-tree dataLoader 消费
- [ ] 2.3 在 `src/lib/file-tree.ts` 中实现 `computeRelativePath(absolutePath, entryPoints)` 工具函数

## 3. UI 组件

- [ ] 3.1 创建 `src/components/transfer/file-tree-item.tsx`：实现 FileTreeItem 组件，支持 5 种 variant（select / transferring / completed / waiting / error），采用 leftGroup(flex-1) + rightGroup 布局
- [ ] 3.2 创建 `src/components/transfer/folder-row.tsx`：实现 FolderRow 组件（chevron + folder 图标 + 名称 | 统计 + 操作），展开态有 accent 背景
- [ ] 3.3 创建 `src/components/transfer/file-tree.tsx`：实现 FileTree 容器组件，内部集成 `useTree`（headless-tree）+ `useVirtualizer`（@tanstack/react-virtual），接收 mode="select" | "transfer"，头部显示标题 + 统计
- [ ] 3.4 实现缩进引导线：根据 `item.getItemMeta().level` 计算 paddingLeft（22px/层），用 CSS 绘制 border-left 引导线
- [ ] 3.5 为所有新组件添加 Lingui i18n 翻译标记（Trans 宏）

## 4. 文件选择状态管理

- [ ] 4.1 创建 `src/hooks/use-file-selection.ts`：实现 useFileSelection Hook，管理 entries Map + entryPoints 数组，返回 headless-tree 所需的 dataLoader / rootChildren / 统计数据
- [ ] 4.2 在 useFileSelection 中实现 `addPaths`：区分文件/文件夹，文件夹调 Tauri 命令枚举子文件，文件直接获取 meta 信息
- [ ] 4.3 在 useFileSelection 中实现 `removePath` + `clear`，移除后自动重新计算 dataLoader 和统计数据

## 5. Tauri 命令（获取文件元信息）

- [ ] 5.1 在 `src/commands/transfer.ts` 中添加 `listFiles(path: string)` 命令 stub（递归枚举文件夹，返回绝对路径 + size + isDirectory）
- [ ] 5.2 在 `src/commands/transfer.ts` 中添加 `getFileMeta(paths: string[])` 命令 stub（批量获取文件元信息）

## 6. 页面集成

- [ ] 6.1 重构 `src/routes/_app/send.lazy.tsx`：用 `useFileSelection()` 替换 `useState<SelectedFile[]>`，用 `FileTree` mode="select" 替换 `FileList`
- [ ] 6.2 更新 `SendContent` 组件：传入 FileTree 所需的 dataLoader / rootChildren / onRemove / 统计数据
- [ ] 6.3 验证桌面端和移动端两种视图下 FileTree 的渲染效果

## 7. 清理

- [ ] 7.1 运行 `pnpm i18n:extract` 提取新增翻译字符串
- [ ] 7.2 确认 TypeScript 编译通过（`pnpm build` 前端构建无报错）
