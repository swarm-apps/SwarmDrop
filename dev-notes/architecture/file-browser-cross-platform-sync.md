# 文件浏览与传输任务布局：SwarmDrop-RN 同步指南

> 状态：桌面端已在 `eac9720` 完成，并已纳入 `v0.7.4`。本文用于在另一台持有
> **最新 SwarmDrop-RN 代码**的电脑上核查和同步相同的产品原则。

> 重要边界：当前电脑的 `swarmdrop-mobile` checkout 较旧且带有未提交生成代码，本文不把它当成
> 移动端现状来源，也不会在这里拉取或修改移动端代码。下文涉及移动端的内容均为“潜在问题、搜索线索和
> 推荐方案”，必须先在最新代码上确认后再实施。

## 1. 结论

这次桌面优化不是单纯换样式，而是把文件展示和任务页布局收敛为两条稳定契约：

1. **所有文件集合使用同一份文件浏览模型和同一个浏览组件。** 树形与网格只是视图，
   发送、接收、传输和收件箱的业务行为由显式能力注入。
2. **任务导航与主要操作固定，只有中间内容滚动。** 文件数量增加、成功摘要变高或窗口变矮时，
   都不能把主要操作和文件明细挤出可视区域。

SwarmDrop-RN 应同步这两个契约，但不应照搬桌面 JSX、Tailwind 尺寸或 Dialog 形态。
移动端需要使用 React Native 的虚拟列表、Safe Area、Bottom Sheet / 全屏页面等原生布局方式实现
“语义一致、交互适配”。

## 2. 桌面端本次做了什么

### 2.1 统一 FileBrowser

桌面端新增 `src/components/file-browser/`，取代旧 `file-tree` 和收件箱页面私有的文件卡片：

```text
FileBrowser
├── Header：标题、文件数、总大小、视图切换
├── FileTreeView：目录层级、展开折叠、虚拟滚动
└── FileGridView：文件卡片、缩略图、状态、按行虚拟滚动
```

页面只传入扁平叶子文件、当前视图和允许的操作，不直接操作树引擎，也不自行实现另一套文件卡片。

### 2.2 统一文件模型与状态

```ts
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
  sourceId?: string | number;
  name: string;
  relativePath: string;
  size: number;
  localPath?: string;
  previewUrl?: string;
  status: FileBrowserStatus;
  progress?: number;
}
```

发送扫描结果、接收 Offer、传输投影和收件箱记录分别通过 adapter 转为该模型。树形目录由
`relativePath` 派生，网格直接渲染叶子文件，因此切换视图不会改变文件集合和业务状态。

### 2.3 文件夹展开不再等于选中

旧文件树会让展开目录长期保持高亮，看起来像“选中了这个目录”。现在展开态只改变：

- chevron 方向；
- `Folder` / `FolderOpen` 图标；
- 子项可见性、缩进和低对比度层级线。

背景只用于 hover、键盘焦点或未来真实 selection。移动端没有 hover，但同样不能用常驻高亮表达展开；
按压反馈应是瞬时的，展开状态由箭头、图标和层级表达。

### 2.4 树形与网格使用同一组操作能力

删除、打开、定位、重试等行为由调用方显式提供：

| 场景 | 能力 |
| --- | --- |
| 发送选择 | 删除文件、按目录前缀移除 |
| 接收 Offer | 只读预览 |
| 传输详情 | 状态、进度，失败时按能力提供重试 |
| 收件箱 | 打开、在文件夹中显示、missing 降级 |

组件不能根据 `tree` / `grid` 或页面名称猜测业务行为。

### 2.5 视图偏好按场景保存

桌面端使用 `send`、`inbox`、`transfer` 三个独立 scope。普通发送和快捷发送共用 `send`；
传输详情、发送进度和接收 Offer 共用 `transfer`。一个场景切换网格，不会意外改变另一个场景。

### 2.6 大文件集合独立滚动并虚拟化

- 树形按可见节点虚拟化；
- 网格按“行”虚拟化，并在容器宽度变化时重新计算列数和行高；
- 视图切换后新视图从顶部开始，不复用不兼容的像素偏移；
- FileBrowser header 和页面主要操作不参与文件列表滚动。

### 2.7 发送、传输和接收页面重新划分滚动职责

桌面端统一为：

```text
固定：全局 / 任务 Header
滚动：摘要 + 文件浏览区域
固定：主要操作栏
```

发送成功页的文件明细使用稳定高度，成功摘要变高时不会把文件列表压缩成一小条。发送选择、配对输入、
配对码页面也迁移到固定 footer。接收 Offer 弹窗改为更宽的紧凑布局，文件区有稳定高度并支持树形 / 网格，
保存位置和接收 / 拒绝按钮固定可达。

### 2.8 预览权限边界留在 adapter

统一组件不会拿任意本地路径自行生成预览 URL。只有调用方确认 URI 可访问时才传 `previewUrl`；
无权限、未落盘或加载失败时统一回退文件类型图标。视图便利不能扩大文件系统暴露范围。

### 2.9 应用壳与任务页保持一致

- 发送流程重新保留全局 AppTopBar，并增加“主页 > 发送文件”上下文，避免发送页像脱离应用的独立窗口；
- 任务内部仍有自己的返回栏，但不重复承担全局状态、主题、收件箱、传输活动和窗口控制；
- Windows 自绘关闭按钮的 destructive 前景色改为高对比色，hover 时关闭图标不再消失；
- 这两项在 RN 上分别对应“Stack / Drawer 导航上下文保持一致”和“使用平台原生窗口控制”，不应复制桌面实现。

## 3. SwarmDrop-RN 需要确认的潜在问题

下面不是对最新移动端代码的结论，而是从桌面端问题反推的核查清单。先定位最新实现，再判断是否仍存在。

| 场景 | 需要确认的问题 | 若存在会造成什么 |
| --- | --- | --- |
| 发送选择 | 是否只显示文件数 / 总大小，没有完整文件列表、删除和视图切换 | 用户无法在发送前发现误选或同名文件 |
| 接收 Offer | 是否只截取前几项，或使用固定高度 `ScrollView` + 全量 map | 大集合不可完整检查，列表性能和滚动边界不稳定 |
| 传输详情 | 是否只有总进度，没有逐文件状态 | 成功 / 失败时无法确认具体文件和失败项 |
| 终态会话 | completed / failed 是否会立即删除 progress 或 session | 成功态文件明细消失，详情页无法回看 |
| 文件组件 | 发送、Offer、详情是否各自实现不同 FileRow / FileCard | 样式、状态、操作和 bug 修复持续漂移 |
| 长列表 | 是否把 `FlatList` 嵌在 `ScrollView`，或直接 map 大量文件 | 虚拟化失效、手势冲突、底部操作被撑出屏幕 |
| 页面布局 | Header / footer 是否与内容共用一个 ScrollView | 小屏、横屏、大字体时主要操作不可达 |
| 视图偏好 | 是否只有全局偏好，或没有 send / transfer / inbox scope | 一个页面切换视图会干扰另一个页面 |
| 预览 URI | 组件是否直接把任意 `sourceId` / 路径当可长期访问 URI | 权限失效、坏图重试、临时 URI 被错误持久化 |

### 3.1 优先核查终态文件上下文是否仍被丢弃

在最新代码中搜索 `registerSession`、`removeSession`、`TransferCompleted`、`TransferFailed`、
`selectedFiles`、`dismissOffer`：

- session 注册时是否保留文件元数据、方向和对端名称；
- 清空发送选择 / dismiss Offer 之前，文件元数据是否已进入会话投影；
- completed / failed 是更新 phase，还是直接删除 progress / session；
- 成功 / 失败详情页能否在事件发生后继续显示文件明细。

若最新实现仍只保留 sessionId 和最新 progress，应先补会话投影，再做 FileBrowser；否则统一组件只能改善
活动态 UI，无法解决桌面端已经修复的成功态问题。

### 3.2 确认 bindings 是否包含重建目录所需字段

检查最新的 `MobileTransferFile`、`MobileTransferOfferFile`、`MobileFileProgress`：

- 是否都有稳定 `fileId` / `sourceId`、`name`、`size`；
- Offer / session metadata 是否保留 `relativePath`；
- progress 若没有 `relativePath`，JS 会话投影是否仍保留原始文件元数据并按 fileId 合并。

只有确认核心层本来就应该持续提供路径时，才扩展 mobile-core mirror 和重新生成 bindings；不要为了 UI
方便无条件修改共享 Rust core。

### 3.3 确认发送来源是否真的包含目录层级

DocumentPicker、MediaLibrary 和 Share Extension 可能提供不同 URI 与路径信息。若最新 adapter 仍只把文件名
作为 `relativePath`，树形视图自然退化为普通列表，这是正确行为。只有来源真的包含相对路径时才建立目录，
不要为了视觉对齐伪造文件夹。

## 4. RN 推荐架构

以下方案仅在最新代码确认存在对应问题后采用；如果 RN 已有等价统一组件或会话投影，应扩展现有能力，
不要另起第二套实现。

### 4.1 新增移动端 FileBrowser，但不共享桌面渲染代码

建议目录：

```text
src/components/file-browser/
├── types.ts
├── adapters.ts
├── file-browser.tsx
├── file-tree-view.tsx
├── file-grid-view.tsx
├── file-row.tsx
├── folder-row.tsx
└── file-card.tsx
```

类型名、状态、scope 和 adapter 职责尽量与桌面一致；渲染层使用 React Native 组件，不抽取跨平台 JSX。
桌面与移动共享的是产品契约，不是 DOM / Native View 实现。

### 4.2 使用 FlatList，不把虚拟列表嵌进 ScrollView

第一阶段不需要新增 FlashList 依赖，React Native `FlatList` 足够：

- 树形：把已展开节点拍平成 visible rows，固定行高，使用 `FlatList`；
- 网格：使用 `FlatList numColumns`，视图或列数变化时通过稳定 `key` 重挂载；
- 固定行高时提供 `getItemLayout`；
- 使用稳定 `keyExtractor`，不要以数组下标作为文件 ID；
- 大集合再调 `initialNumToRender`、`windowSize` 和 `maxToRenderPerBatch`；
- 在 Bottom Sheet 内使用对应的 `BottomSheetFlatList`，不要再包一层 `BottomSheetScrollView`。

禁止结构：

```text
ScrollView
└── FlatList / 大量 map(file)
```

这会关闭或削弱虚拟化，并让手势、滚动高度和底部按钮可达性变得不可靠。

### 4.3 先建立 SessionProjection，再接 FileBrowser

建议把 `transfer-store` 从“活动 ID + 最新 progress”提升为会话投影：

```ts
interface MobileTransferProjection {
  sessionId: string;
  direction: "send" | "receive";
  peerName: string;
  phase: "waiting" | "active" | "paused" | "completed" | "failed";
  files: FileBrowserItem[];
  totalBytes: bigint;
  transferredBytes: bigint;
  error?: string;
  completedAt?: number;
}
```

调用约定：

1. 发送：`registerSession(result.sessionId, selectedFiles, peerName)`，再清空选择；
2. 接收：`registerSession(current.id, current.offer.files, deviceName)`，再 dismiss Offer；
3. progress：按 `fileId` 更新文件状态和进度，不替换元数据；
4. completed / failed：更新 phase，**不要删除 projection**；
5. 真正清理由历史保留策略、用户删除或数据库投影决定。

### 4.4 RN adapter 建议

| Adapter | 输入 | 注意点 |
| --- | --- | --- |
| `fromSelectedFiles` | `MobileTransferFile[]` | `sourceId` 作为稳定来源；默认 idle；URI 不自动变预览 |
| `fromOfferFiles` | `MobileTransferOfferFile[]` | 忽略目录 marker，以叶子文件和 relativePath 构树 |
| `mergeTransferProgress` | projection files + `MobileFileProgress[]` | 按 fileId 合并，不丢 relativePath |
| `fromInboxFiles` | 未来收件箱 / 历史 DTO | 只有确认可访问的本地 URI 才提供 previewUri |

### 4.5 RN 视图和偏好

概念上仍使用 `tree | grid`：移动端 tree 是紧凑的层级列表；没有目录时就是普通文件列表。

建议 scope：

| scope | 首次默认 | 说明 |
| --- | --- | --- |
| send | tree | 通用文件名和大小优先；用户可切网格检查媒体 |
| transfer | tree | Offer 和逐文件进度优先保证密度 |
| inbox | grid | 等 RN 真正实现收件箱时启用 |

偏好写入现有 AsyncStorage `preferences-store`。只影响展示，不进入 `mobile-core-store` 或 Rust core。

### 4.6 RN 页面高度契约

普通任务页：

```text
SafeAreaView flex: 1
├── 固定 Header
├── View flex: 1
│   └── FileBrowser / 其他 FlatList（唯一主滚动区）
└── 固定 Footer + bottom safe-area padding
```

传输成功 / 失败页：摘要和稳定高度文件区可共同处于中间滚动区，但主要操作必须在外部固定。若手机高度不足，
优先让摘要 + 文件区滚动，不能压缩文件区到不可用，也不能让“完成 / 重试 / 发送更多”滚出屏幕。

### 4.7 接收 Offer 需要响应式重设计

当前 86% 宽、最大 480px 的居中 AlertDialog 适合短摘要，不适合可切换的网格文件浏览器。建议：

- 手机：使用接近全高的 Bottom Sheet 或全屏 modal；
- 平板 / 大屏：使用宽版居中 Dialog；
- header 固定：来源设备、文件数、总大小；
- 中间 FileBrowser 独立滚动；
- footer 固定：拒绝、接收；
- 接收前没有本地文件 URI，卡片只显示类型图标；
- 不再只显示前 5 项，统计与列表都使用完整叶子文件集合。

### 4.8 移动端预览安全与生命周期

RN 端的 `sourceId` 可能是 `content://`、`file://` 或媒体库 URI，不能假定都可长期访问：

- FileCard 只消费显式 `previewUri`；
- 选择器返回 URI 是否可预览，由 adapter 判断；
- Offer 在接收前没有本地 URI，只显示文件类型图标；
- 落盘后的 URI 只有在宿主确认仍有权限时才能用于预览；
- 图片加载失败回退类型图标，不持续重试；
- 不把临时 picker URI 当成可持久化历史路径。

## 5. 建议实施顺序

### Phase 0：修正会话投影

- 扩展 `registerSession`，保留文件元数据、方向和对端名称；
- completed / failed 改为更新终态，不立即删除；
- 明确内存投影与未来数据库历史的边界；
- 为 progress 合并和终态保留补单元测试。

### Phase 1：建立 RN FileBrowser

- 定义统一 item / status / action / scope；
- 实现 selected / offer / progress adapters；
- 实现 FlatList 树形和网格；
- 实现标题、统计、切换、空状态和显式 actions；
- 增加 AsyncStorage 场景偏好。

### Phase 2：迁移发送流程

- `select-device.tsx` 不再只显示数量；
- 用户可查看完整已选文件、切换视图和移除误选文件；
- 设备列表与文件浏览器不要形成两个竞争的纵向 ScrollView；可采用分步页、折叠文件摘要或独立文件抽屉；
- 发送成功后进入可保留终态的详情，而不是直接回退且丢失上下文。

### Phase 3：迁移接收 Offer

- 用响应式 Bottom Sheet / Dialog 容器承载完整 FileBrowser；
- 文件区域虚拟化并支持 grid；
- 接收 / 拒绝固定；
- Offer 队列切换时重置滚动和临时 UI 状态。

### Phase 4：迁移传输详情

- 渲染 projection 的完整文件状态；
- 支持 tree / grid；
- 成功、失败、暂停状态都保留文件明细；
- 固定底部操作；
- 接收完成后再接系统“打开 / 分享 / 查看保存位置”能力。

### Phase 5：未来收件箱统一

RN 当前没有与桌面同等的收件箱详情。后续实现时直接使用 FileBrowser，不再创建第三套卡片组件。

## 6. 验收清单

### 统一性

- [ ] 发送、Offer、传输详情都只通过 RN FileBrowser 展示文件集合
- [ ] 不存在页面私有 FileCard 或第二套文件列表状态映射
- [ ] tree / grid 切换不改变文件集合、进度和操作能力
- [ ] send / transfer / inbox 偏好互不覆盖

### 大文件集合

- [ ] 1、100、1,000、10,000 个文件均不会一次性挂载全部 item
- [ ] 列表和网格都能滚到底部
- [ ] 切换视图后滚动位置可预测
- [ ] 展开深层目录不会让行宽或操作按钮不可达
- [ ] 不存在 ScrollView 嵌套 FlatList 的警告

### 页面布局

- [ ] Header 和主要操作不随文件列表滚走
- [ ] 小屏、横屏、键盘弹出和大字体下主要操作仍可达
- [ ] 成功摘要变高时文件区仍保持可用高度
- [ ] Offer 在手机和 Tablet 上都能使用网格，不挤压接收 / 拒绝按钮
- [ ] Safe Area 下 footer 不被 Home Indicator / 系统导航条遮挡

### 状态与数据

- [ ] waiting、transferring、completed、error、missing 视觉语义一致
- [ ] completed / failed 不会立即删除 session projection
- [ ] 发送选择和 Offer 元数据能与 progress 按 fileId 合并
- [ ] 同名不同路径文件使用稳定且不同的 key
- [ ] 目录移除不会误删相似前缀文件

### 可访问性和触摸

- [ ] 视图切换暴露选中状态和可访问名称
- [ ] 文件夹行可访问并正确暴露 expanded 状态
- [ ] 点击次要操作不会误触文件主操作
- [ ] 触摸目标至少满足移动端最小尺寸
- [ ] 屏幕阅读器能读出文件名、大小、状态和进度

### 预览与权限

- [ ] 只有显式可访问 URI 才渲染缩略图
- [ ] Offer 接收前不会尝试打开远端路径
- [ ] 临时 picker URI 不会被错误持久化为永久路径
- [ ] 图片失败、权限失效和文件缺失都能回退图标

## 7. 需要一并审计的相似页面

同步 FileBrowser 时顺便检查以下页面的“固定导航 / 中间滚动 / 固定操作”关系：

- `src/components/pairing-sheet.tsx`：输入码、错误提示和键盘出现后，主要操作是否仍可达；
- `src/components/update-host.tsx`：长 release notes 的 ScrollView 是否与底部升级操作隔离；
- `src/components/node-control-sheet.tsx`：动态内容变高时主要操作是否被滚走；
- 后续新增的收件箱、分享目标和恢复传输页面。

这不意味着它们必须复用 FileBrowser，而是要复用相同的任务页滚动契约。

## 8. 桌面参考实现

- 统一组件说明：`dev-notes/knowledge/file-browser.md`
- OpenSpec 设计：`openspec/changes/unify-file-browser-views/design.md`
- 统一组件：`src/components/file-browser/`
- 任务页布局：`src/components/layout/task-surface.tsx`
- 发送成功页：`src/routes/_app/send/-components/send-progress-view.tsx`
- 接收 Offer：`src/components/transfer/transfer-offer-dialog.tsx`
- 传输详情：`src/components/transfer/session-panel.tsx`

## 9. 不要机械同步的桌面细节

- Windows 自绘标题栏和关闭按钮 hover 修复与 RN 无关；
- 桌面 AppTopBar / 面包屑应映射为 RN Stack Header，而不是复制结构；
- Tauri asset protocol 应映射为 Expo / 平台 URI 权限边界；
- `@headless-tree/react` 和 `@tanstack/react-virtual` 不应带到 RN；
- 桌面固定像素高度只能作为视觉意图，RN 应结合 Safe Area、屏幕高度和字体缩放实现。

真正需要跨端保持一致的是：数据语义、状态语义、显式能力、视图偏好隔离、长列表虚拟化和任务操作可达性。

## 10. 在另一台电脑上的执行流程

### 10.1 先获取最新代码

在 SwarmDrop-RN 仓库执行：

```powershell
git status --short
git branch --show-current
git fetch origin --prune
git pull --ff-only
```

如果 `git status` 非空，先判断改动归属；不要为了拉取而直接 reset 或覆盖本地工作。

### 10.2 建立最新代码映射

```powershell
rg --files src | rg "send|transfer|inbox|receive|offer|file|progress|preferences|store"
rg -n "ScrollView|FlatList|FlashList|SectionList|numColumns" src
rg -n "registerSession|removeSession|TransferCompleted|TransferFailed|selectedFiles|dismissOffer" src
rg -n "MobileTransferFile|MobileTransferOfferFile|MobileFileProgress|relativePath" src packages
```

把最新实现映射到以下五类入口：

1. 发送前文件检查；
2. 接收 Offer；
3. 活动传输详情；
4. 成功 / 失败终态；
5. 收件箱 / 历史（若已实现）。

### 10.3 先输出核查结论，再决定改动

建议在另一台电脑先产出一张表：

| 场景 | 最新组件路径 | 文件数据来源 | 列表技术 | 终态是否保留 | 是否需要调整 |
| --- | --- | --- | --- | --- | --- |
| 发送选择 |  |  |  |  |  |
| 接收 Offer |  |  |  |  |  |
| 传输详情 |  |  |  |  |  |
| 收件箱 / 历史 |  |  |  |  |  |

只有表中确认的问题进入实现范围。若需要大改，建议先建立独立 OpenSpec change，避免把状态模型、统一组件、
页面重构和共享 core 变更混在一个不可验证的提交里。

### 10.4 实施后的最低验证

```powershell
pnpm typecheck
pnpm lint:ci
pnpm i18n:extract
```

同时在 iOS 和 Android 至少验证：大量文件、深层目录、树 / 网格切换、Offer、传输中、成功、失败、横屏、
大字体和系统返回手势。组件测试通过不能代替真实设备上的滚动与 Safe Area 验证。
