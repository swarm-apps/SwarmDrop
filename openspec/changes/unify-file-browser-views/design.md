## Context

SwarmDrop 当前有两条文件展示实现：

- `src/components/file-tree/` 使用 `@headless-tree/react` 和 `@tanstack/react-virtual`，服务发送选择、快捷发送、接收 Offer 与传输详情。它接受 `TreeDataLoader`、`rootChildren` 和 `mode`，树形能力完整，但 `mode` 同时承担业务语义与视觉状态，调用方难以复用其他布局。
- `src/routes/_app/inbox/index.lazy.tsx` 内部定义 `FileCard`，收件箱详情直接映射文件数组生成网格。它支持图片缩略图、打开、定位和缺失状态，但无法复用树形层级、虚拟化和其他传输状态。

现有 `file-tree-component` OpenSpec change 仍显示 0/20 任务，但主要组件已经存在，且其“展开文件夹使用 accent 背景”等要求已与当前设计判断冲突。本变更以新的 `file-browser-ui` capability 描述实际目标；实现完成后应对账并关闭旧变更，避免两个未完成契约并存。

约束：

- 桌面端 React 19 + Tailwind CSS 4 + Lingui 5，不新增 UI 或图标体系。
- 保留 `@headless-tree/react` 与 `@tanstack/react-virtual`。
- 文件集合可能非常大，树形和网格都不能一次性渲染全部条目。
- Tauri asset protocol 当前只允许受控目录。发送选择可以访问任意本地路径，但统一组件不能以预览为由扩大文件系统暴露范围。
- 组件必须同时适应整页面板、窄栏、抽屉和限高 Dialog。

## Goals / Non-Goals

**Goals:**

- 建立一个对外统一的 `FileBrowser`，以同一数据模型驱动树形与网格视图。
- 将文件集合、显示状态、可执行操作和布局选择相互解耦。
- 精修树形层级表达，移除“展开等于高亮”的错误视觉语义。
- 抽取收件箱卡片，实现发送、收件箱和传输场景的卡片式查看。
- 让两种视图都支持独立滚动、虚拟化、键盘操作、暗色主题和状态反馈。
- 按场景保存视图偏好，同时保留场景合理的默认视图与可用视图限制。

**Non-Goals:**

- 不新增后端命令、数据库字段或传输协议字段。
- 不在第一阶段实现网格中的文件夹钻取、面包屑导航、重命名、排序、拖拽重排或多选。
- 不为任意本地文件放宽 Tauri asset protocol scope。
- 不实现通用媒体播放器、PDF 预览或文档内容预览。
- 不把 `FileBrowser` 扩展为操作系统级文件管理器。

## Decisions

### 1. 对外组件提升为 FileBrowser，FileTree 降为内部视图

新增 `src/components/file-browser/`：

```text
FileBrowser
├── FileBrowserHeader
│   ├── title + statistics
│   └── FileViewToggle
├── FileTreeView
│   ├── FolderRow
│   └── FileRow
└── FileGridView
    └── FileCard
```

页面只导入 `FileBrowser`、类型和适配器，不再直接组装 `FileTree` 或私有 `FileCard`。现有 `src/components/file-tree/` 的数据构建、树引擎和行组件迁移到新模块内部；迁移期间可以保留临时 re-export，所有调用点迁完后删除。

理由：树形只是文件浏览的一种布局。继续向 `FileTree` 添加网格和收件箱行为会让名称、API 和职责失真。

备选方案：给 `FileTree` 增加 `layout="grid"`。拒绝，因为树引擎不应成为网格数据与操作的唯一入口。

### 2. 使用扁平叶子文件模型，目录由树适配器派生

统一模型以文件叶子为事实源：

```typescript
type FileBrowserStatus =
  | "idle"
  | "waiting"
  | "transferring"
  | "completed"
  | "error"
  | "missing";

interface FileBrowserItem {
  id: string;
  fileId?: number;
  name: string;
  relativePath: string;
  size: number;
  localPath?: string;
  previewUrl?: string;
  status?: FileBrowserStatus;
  progress?: number;
}
```

树形视图从 `relativePath` 合成目录节点、目录累计大小和文件数量；网格视图直接消费叶子文件，因此切换视图不会改变集合。目录 ID 由规范化相对路径生成，文件 ID 由调用方稳定 ID 或命名空间加相对路径生成。

适配器放在 `src/components/file-browser/adapters.ts`，至少覆盖：

- `fromEnumeratedFiles`：发送选择与快捷发送。
- `fromTransferProjectionFiles`：传输详情与发送进度。
- `fromOfferFiles`：接收 Offer。
- `fromInboxFiles`：收件箱详情。

理由：现有所有后端 DTO 都能提供文件名、相对路径和大小，额外状态是可选增强。统一模型不应反向污染后端类型。

备选方案：继续让页面构造 `TreeDataLoader`。拒绝，因为这会让网格视图依赖树内部结构，并在各页面重复适配逻辑。

### 3. 视图类型与业务能力分离

`FileBrowser` 使用显式布局与操作能力：

```typescript
interface FileBrowserProps {
  items: FileBrowserItem[];
  view: "tree" | "grid";
  onViewChange?: (view: FileBrowserView) => void;
  availableViews?: FileBrowserView[];
  title: ReactNode;
  empty?: ReactNode;
  onRemove?: (itemOrDirectory: FileBrowserTarget) => void;
  onOpen?: (item: FileBrowserItem) => void;
  onReveal?: (item: FileBrowserItem) => void;
  onRetry?: (item: FileBrowserItem) => void;
}
```

视图只渲染调用方提供的操作。状态来自 item，不再由 `mode="select" | "transfer"` 推断业务行为。目录删除目标包含目录相对路径前缀，以保持当前发送页一次移除目录的能力。

理由：同一个 completed 文件在收件箱需要打开 / 定位，在传输详情可能只读；行为来自上下文，不来自树形或网格布局。

### 4. 树形展开不使用常驻行背景

`FolderRow` 的 expanded 状态只改变：

- chevron 方向；
- `Folder` / `FolderOpen` 图标；
- 子项可见性；
- 缩进和低对比度引导线。

默认、折叠和展开行背景都透明；hover 使用轻微 surface tint，focus-visible 使用 ring，只有未来真实 selection 状态才允许持续选中背景。次要操作在 hover / focus-within 时增强可见度，但保留键盘可达性。

文件类型图标复用 `src/lib/file-icon.ts`，树形行根据传输状态覆盖必要的语义色，不再让“展开”承担“选中”的视觉含义。

### 5. 网格第一阶段展示扁平文件卡，不实现目录导航

网格按叶子文件呈现，卡片包含：

- 4:3 预览区域；
- 图片缩略图或文件类型图标 fallback；
- 文件名、大小；
- 同名或存在层级时显示精简相对目录；
- missing、transfer status、progress 等状态；
- 调用方提供的主操作与次要操作。

文件夹层级管理仍由树形视图负责。网格若渲染文件夹卡，就必须同时引入当前目录、返回、面包屑和目录聚合状态，会显著扩大本次范围。

卡片圆角使用 14px，操作按钮使用现有小控件尺度，外层 `glass-panel` 内不再叠加重阴影卡片。图片错误后回退图标，避免坏图占位。

### 6. 缩略图 URL 由调用方或适配器显式提供

`FileCard` 不接收任意路径后自行调用 `convertFileSrc`。只有 `previewUrl` 存在时才尝试加载图片。

- 收件箱适配器可对安全范围内、非 missing 的图片生成 `previewUrl`。
- 发送选择虽然拥有文件绝对路径，但第一阶段不扩大 asset scope，默认显示文件类型图标。
- 传输投影与 Offer 没有可靠本地路径，默认显示类型图标。

理由：将安全边界放在数据适配层，展示组件无需理解 Tauri 路径权限，也不会意外暴露任意本地文件。

备选方案：扩大 asset scope 到任意路径。拒绝，因为预览便利不足以证明更大的文件系统暴露面合理。

### 7. 树形虚拟化按节点，网格虚拟化按行

树形继续使用当前可见节点数组和单行 virtualizer。网格使用容器宽度计算列数，将文件数组切为虚拟行：

```text
columnCount = floor((containerWidth + gap) / (minCardWidth + gap))
rowCount = ceil(items.length / columnCount)
```

virtualizer 只虚拟纵向行，每个虚拟行内部渲染固定数量卡片。使用 `ResizeObserver` 或现有 React Virtual 测量能力响应容器宽度变化。切换视图时滚动容器回到顶部，不尝试在结构不同的布局间复用像素偏移。

所有外层保持 `flex min-h-0 flex-1 flex-col`，实际滚动只发生在视图内容区，header 与页面底部命令栏不参与滚动。

发送进度页需要比通用 FileBrowser 多一层页面级滚动约束：`TaskToolbar` 与 `CommandDock` 位于中间滚动区之外，成功摘要和文件明细面板放入同一个 `overflow-auto` 区域。文件明细面板使用稳定的响应式高度（小屏 360px、桌面最高 440px），避免终态摘要变高时挤压文件浏览区。

同一高度链规则由 `TaskContent.footer` 复用到发送选择、配对输入与配对码页面：footer 固定在任务页底部，只有中间内容滚动。快捷发送原本已使用中间面板伸缩加固定命令栏，不重复套用。传输详情宽屏右栏采用相同结构，文件明细保持 360–400px 稳定高度，操作区固定在详情面板底部；窄屏仍遵循 MasterDetailShell 的整页滚动策略。

备选方案：CSS Grid 直接 map 全量文件。拒绝，因为会退化现有大文件夹能力。

### 8. 视图偏好按场景持久化

在 `preferences-store` 增加：

```typescript
type FileBrowserScope = "send" | "inbox" | "transfer";

fileBrowserViews: Record<FileBrowserScope, FileBrowserView>;
setFileBrowserView(scope, view): void;
```

默认值：

| scope | default |
| --- | --- |
| send | tree |
| inbox | grid |
| transfer | tree |

快捷发送与普通发送共用 send scope；传输活动详情、发送进度与接收 Offer 共用 transfer scope。接收弹窗使用更宽的紧凑布局和稳定高度文件区，确保网格卡片不会挤压保存路径与确认操作。组件若发现已保存视图不在 `availableViews` 中，使用第一个允许视图但不覆盖偏好。

理由：用户对收件箱和传输进度的密度偏好可能不同，一个全局 viewMode 会在场景间产生意外切换。

### 9. 分阶段迁移并保持自动化选择器稳定

迁移顺序：

1. 建立统一类型、适配器、FileBrowser shell 和 refined tree view。
2. 迁移 `/send` 与 `/send/share-target`，验证删除、滚动和网格切换。
3. 迁移收件箱，抽取缩略图逻辑并删除页面私有 `FileCard`。
4. 迁移 `SessionFileSection`，覆盖传输页与发送进度。
5. 迁移接收 Offer，支持 tree / grid 并重构为适合网格的宽版紧凑弹窗。
6. 删除旧 re-export 和过时实现。

保留现有关键 `data-testid`，并为 view toggle、grid、card 增加稳定测试标识，避免现有桌面 E2E 演示失效。

## Risks / Trade-offs

- **[风险] 统一 API 变成大量可选 props** -> 将文件数据、视图状态和 actions 分组为小型类型，页面通过适配器与能力对象组装，避免 context 巨型 switch。
- **[风险] 网格虚拟行在窗口缩放时跳动** -> 固定卡片纵横比和元数据区高度，列数变化时重新测量并回到可预测位置。
- **[风险] 网格状态无法表达目录聚合进度** -> 第一阶段只展示叶子文件状态；目录聚合只在树形视图呈现。
- **[风险] 图片缩略图加载造成内存或权限问题** -> 仅接收显式 preview URL、lazy load、失败回退；不扩大 asset scope。
- **[风险] 旧 change 与新 capability 重叠** -> 实现完成后对账 `file-tree-component` 已落地事项并将其标记为被本变更取代，归档时只保留新的统一 capability。
- **[风险] 各页面默认视图和操作漂移** -> 用 scope 默认表和场景集成测试固定行为。
- **[权衡] 网格不支持目录钻取** -> 保持本次可控；树形用于层级管理，网格用于视觉检查。未来若真实需求成立，再单独设计目录导航。

## Migration Plan

1. 先添加新模块和测试，不删除现有 `FileTree` / `FileCard`。
2. 逐个调用点切换到 `FileBrowser`，每迁移一个场景就验证相同文件、统计、操作和滚动。
3. 在所有页面迁移完成后删除收件箱私有卡片与旧 file-tree 对外入口。
4. 提取 Lingui 文案并验证亮色、暗色、树形、网格、窄栏、抽屉和 Dialog。
5. 对账旧 `file-tree-component` OpenSpec change，避免重复归档规格。

回滚时可以在迁移阶段按调用点恢复旧组件；后端和持久业务数据没有迁移，唯一新增持久值是可安全忽略的视图偏好。

## 与旧 file-tree-component 变更的对账结果

`file-tree-component` 中已经落地且仍有效的能力（headless-tree、节点虚拟化、目录层级、缩进引导线、选择删除和传输状态）已迁入 `file-browser` 内部树形视图。旧变更要求的“展开目录使用 accent 背景”被本变更明确取代，文件夹展开只改变 chevron 和文件夹图标；旧的 mode 驱动 API 也由统一状态模型与显式 actions 取代。

因此旧 change 不应再单独实现或归档为主规格；后续以本变更的 `file-browser-ui` capability 为唯一文件展示契约。

## Open Questions

- 第一阶段是否需要为发送选择生成安全缩略图服务。默认答案为否，先使用类型图标；若后续需要，应单独设计限尺寸、限格式、可取消的 host thumbnail command。
- 网格中是否需要目录级批量删除入口。默认保留树形目录删除和网格单文件删除，待真实使用反馈后再决定是否增加顶层来源分组。
