## Context

移动端当前已经采用 projection-first 传输架构：Rust/mobile-core 暴露并持久化 `MobileTransferProjection`，RN `transfer-store` 只缓存 projection 与高频 progress overlay。这个事实源能够在完成、失败、暂停和重启后继续提供文件元数据，因此本 change 不需要文档草案中设想的 JS `registerSession` 模型。

文件展示层仍处于迁移前状态：

- `src/components/file-tree/` 同时承担树构建、状态映射和渲染，并以 `virtualize=false` 为默认值；所有现有调用都走全量 `map`。
- 发送选择、系统分享发送、Offer、传输详情和收件箱详情分别维护不同的文件行或不同的滚动容器。
- 普通 picker、媒体库和 Share Intent 会把文件平铺为文件名，当前按 `relativePath` 去重会让不同来源的同名文件静默冲突。
- 传输详情把所有非 transferring 状态映射成 select 模式，projection 虽被保留，终态逐文件语义却没有正确展示。
- `@headless-tree/*` 只被当前 FileTree 使用；其展开状态需要额外 `forceUpdate` 才能刷新。项目已经依赖 FlashList 和 gorhom bottom sheet。

目标结构如下：

```text
MobileTransferFile / Offer / Projection / Inbox DTO
                         │
                         ▼
              ┌─────────────────────┐
              │ FileBrowser adapters│
              │ identity + status   │
              └──────────┬──────────┘
                         ▼
              FileBrowserItem[] + actions
                         │
              ┌──────────┴──────────┐
              ▼                     ▼
       tree visible rows        grid leaf rows
              │                     │
              └──────────┬──────────┘
                         ▼
           FlashList / BottomSheetFlatList
```

## Goals / Non-Goals

**Goals:**

- 所有移动端文件集合使用统一 item、adapter、状态和显式 actions。
- 不同来源的同名文件可以同时选择、展示和发送；重复选择同一来源仍会去重。
- tree/grid 只改变投影方式，不改变文件集合、业务状态或可用操作。
- 大文件集合始终通过非嵌套虚拟列表渲染，任务 header/footer 与 Safe Area 稳定。
- projection 与实时 progress 能产生一致的逐文件活动态和终态状态。
- 保留现有收件箱打开/分享、媒体预览、传输恢复和系统分享发送行为。

**Non-Goals:**

- 不修改共享 Rust 传输协议、数据库 schema 或 mobile-core projection API。
- 不创建 JS 自有的持久 SessionProjection，不把视图偏好放进 mobile-core。
- 不实现跨端共享 JSX、跨设备同步视图偏好或新的收件箱业务能力。
- 不把 picker 临时 URI 升格为永久历史路径，不为 Offer 接收前内容生成缩略图。
- 不在本 change 中重新设计设备卡片、传输协议重试策略或媒体预览器。

## Decisions

### D1. 稳定身份与相对路径分离

统一叶子模型采用移动端专用类型，不直接复制桌面 DOM 类型：

```ts
type FileBrowserStatus =
  | "idle"
  | "waiting"
  | "transferring"
  | "paused"
  | "completed"
  | "cancelled"
  | "error"
  | "missing";

interface FileBrowserItem {
  id: string;
  fileId?: number;
  sourceId?: string;
  name: string;
  relativePath: string;
  size: bigint;
  status: FileBrowserStatus;
  progress?: number;
  previewUri?: string;
}
```

- selected file 的 `id` 从完整 `sourceId` 派生；相同来源再次选择视为重复，不同来源即使同名也必须保留。
- Offer/projection 的 `id` 使用 `sessionId + fileId`，保证 fileId 的会话内作用域不会互相碰撞，并在 Offer → projection 过渡中保持稳定。
- Inbox 使用 native DTO 的稳定 file id，并加 inbox item 作用域。
- `relativePath` 经过统一的分隔符和空段归一化，只负责目录结构、路径展示和目录级操作，不再充当叶子身份。
- 目录节点使用 `dir:<normalized-path>/`，文件节点使用 `file:<item.id>`；同一路径下两个同名叶子仍可并存。

目录移除按路径 segment 边界匹配：只移除 `path === dir` 或 `path.startsWith(dir + "/")` 的叶子，不能把 `foo/` 与 `foobar/` 混为一组。

备选方案是继续用 `relativePath` 作为 ID，但它无法表示平铺来源中的同名文件，故否决。

### D2. adapters 是业务数据与展示模型的唯一边界

新增四类 adapter：

| Adapter | 输入 | 责任 |
| --- | --- | --- |
| `fromSelectedFiles` | `MobileTransferFile[]` | 用 sourceId 建立稳定身份，状态为 idle，不自动开放预览 |
| `fromOfferFiles` | sessionId + `MobileTransferOfferFile[]` | 忽略目录 marker，以叶子 fileId/relativePath 建树，状态为 waiting |
| `fromProjection` | `MobileTransferProjection` + 可选实时 progress | projection 保留元数据，progress 只覆盖实时字节和状态 |
| `fromInboxFiles` | inbox item id + files | 映射 missing/open 能力，只有明确可访问的 URI 才提供 previewUri |

projection 状态合并规则：

1. 实时 progress 存在时，按 fileId 覆盖 transferring/completed 和百分比，不替换 name/path/size。
2. terminal completed 时所有叶子为 completed。
3. terminal fatal error 时，已完成叶子保持 completed，其余叶子为 error。
4. terminal cancelled/rejected 时，已完成叶子保持 completed，其余叶子为 cancelled。
5. suspended 时，已完成叶子保持 completed，有部分进度的叶子为 paused，其余叶子为 waiting。

页面不得重新实现这些映射。RN store 仍以 core projection 为事实源；terminal 时清掉 progress overlay 是允许的，因为 adapter 可从 projection 恢复终态。

### D3. 用纯树构建与显式展开状态替代 headless-tree

`tree-data.ts` 负责把叶子构建成不可变目录树，`flattenVisibleNodes(tree, expandedIds)` 负责生成虚拟列表 rows。展开集合由 FileBrowser 本地状态维护，数据集变化时只保留仍存在的目录 id。

这样可以：

- 删除当前依赖可变 tree state 的 `forceUpdate`；
- 直接单元测试目录计数、排序、重复路径、展开/折叠和深层目录；
- 让 tree/grid 共用同一 `FileBrowserItem[]`，而不是各自持有数据源。

迁移完成后移除仅在旧 FileTree 使用的 `@headless-tree/core` 和 `@headless-tree/react`。备选方案是继续包装 headless-tree，但它在 RN 当前用法中没有提供足以抵消状态复杂度的能力，故否决。

### D4. FileBrowser 接收能力，不接收业务 mode

FileBrowser 不使用 `mode="select" | "transfer"` 猜行为，而是接收可选能力：

```ts
interface FileBrowserActions {
  removeItem?: (item: FileBrowserItem) => void;
  removeDirectory?: (relativeDirectory: string) => void;
  openItem?: (item: FileBrowserItem) => void;
  revealItem?: (item: FileBrowserItem) => void;
  retryItem?: (item: FileBrowserItem) => void;
}
```

缺失回调就不渲染对应操作。行、卡片和目录节点只派发 item/path，不 import store、router 或 native file API。既有收件箱打开/分享和传输重试逻辑继续由页面注入。

### D5. 每个任务面只有一个纵向滚动所有者

普通页面使用 FlashList；Offer bottom sheet 使用 gorhom 的 `BottomSheetFlatList`。两种 renderer 共用 rows、renderItem、keyExtractor 和 actions，不使用已标记 deprecated 的 `BottomSheetFlashList`。

- 发送选择：FileBrowser 占据 header 与固定 footer 之间的 `flex: 1` 区域。
- 系统分享发送：设备列表保留唯一主列表；文件区域显示紧凑摘要，并通过“查看文件”打开独立的全屏 FileBrowser，以免设备列表和文件列表竞争滚动。
- 传输详情：摘要、进度和诊断作为同一个虚拟列表的 header，文件 rows 紧随其后；操作栏在列表外固定。
- 收件箱详情：保留单文件富媒体预览；多文件清单由同一虚拟列表承载，不再在 ScrollView 内 map 私有 FileRow。
- Offer：来源/策略固定在顶部，BottomSheetFlatList 占中间，拒绝/接收作为 footer 固定。

tree/grid 切换或 grid 列数变化时使用包含视图和列数的稳定 list key 重挂载，并把新视图定位到顶部；不复用另一视图的像素 offset。

### D6. 视图偏好按产品场景持久化

`preferences-store` 增加：

```ts
type FileBrowserScope = "send" | "transfer" | "inbox";
type FileBrowserView = "tree" | "grid";
fileBrowserViews: Record<FileBrowserScope, FileBrowserView>;
```

默认值为 send/tree、transfer/tree、inbox/grid。普通发送与 Share Target 文件检查共用 send；Offer 与传输详情共用 transfer；收件箱使用 inbox。persist 的 `partialize` 与 `merge` 必须验证 enum 和缺失 scope，兼容旧存储。

### D7. Offer 使用响应式容器，但不分裂内容实现

`useWindowDimensions` 以 768dp 为 tablet/大屏基线：

- 小于 768dp：使用接近全高的 AppBottomSheet，内部使用 BottomSheetFlatList 与固定 footer。
- 大于等于 768dp：使用宽版居中 Dialog，内部使用普通虚拟列表与最大高度约束。

两种容器渲染同一个 OfferContent/FileBrowser，不复制策略、保存位置和操作逻辑。切换 `currentOffer.id` 时重置保存目录覆盖、滚动位置、展开状态和临时视图状态；持久化的 transfer scope 偏好不重置。

### D8. 预览 URI 必须由 adapter 显式授权

FileCard 只消费 `previewUri`，不读取 `sourceId`、`localPath` 或 relativePath 猜测 URI。Offer 接收前永远没有 previewUri。图片加载失败后本次挂载回退为文件类型图标，不循环重试；临时 picker URI 不进入持久偏好或历史模型。

### D9. 以数据边界测试为主，真实设备验证布局

项目测试基线是 `e2e/webdriver` 下的 WebdriverIO + Appium XCUITest。本 change 沿用该体系，通过可控 fixture、稳定 testID/accessibility id 和屏幕状态断言覆盖文件身份、状态、视图与布局，不引入第二套 Jest/Testing Library runner。

自动化覆盖：

- fixture 驱动的同名来源、目录边界、Offer marker、projection/progress 合并和终态状态；
- tree/grid、actions、可访问状态、视图切换、深层目录和大集合首尾可达；
- preferences scope 隔离、默认值与应用重启后恢复；
- 五个入口的文件集合、固定动作区和导航行为。

真实 iOS/Android 至少验证 1、100、1,000、10,000 文件、小屏、横屏、大字体、键盘、Safe Area、Offer 队列切换和系统返回手势。组件测试不能替代这些交互验证。

## Risks / Trade-offs

- [Risk] grid 缩略图增加解码与内存压力 → 仅对显式 previewUri 渲染，限制卡片尺寸，失败立即回退图标，并用虚拟窗口控制并发挂载。
- [Risk] 一次迁移五个入口改动面较大 → 先交付模型/adapters 和 FileBrowser，再按发送、Offer、传输、收件箱顺序迁移，每一步保持类型检查与页面测试可运行。
- [Risk] FlashList 与 BottomSheetFlatList 的行为细节不同 → 把数据/row/actions 保持统一，只将滚动容器作为内部 adapter；为两种容器分别做集成测试。
- [Risk] projection 缺少逐文件失败原因 → 只按已完成字节与 session terminal reason 映射可证明的状态，不虚构文件级错误文案。
- [Risk] relativePath 归一化改变旧目录展开 key → 展开态不持久化，迁移只影响临时 UI，不影响传输或历史数据。
- [Risk] 纯数据边界没有独立单元 runner → 通过确定性 fixture 把关键结果暴露到真实页面和 accessibility 属性，由 WebDriver 验证用户可观察行为；typecheck/lint 继续约束内部类型与静态边界。

## Migration Plan

1. 扩展现有 WebDriver fixture/页面驱动能力；新增类型、身份/路径工具、adapters、projection 状态合并和纯 tree 数据层，并用端到端状态断言锁定用户可观察行为。
2. 新增 FileBrowser tree/grid、toolbar、rows/cards、虚拟容器 adapter 与 scope 偏好；旧 FileTree 暂时保留。
3. 先迁移发送选择，并同时把 selectedFiles 去重/移除改为稳定身份与 segment 边界。
4. 为 Share Target 增加文件检查入口并迁移私有 SharedFileRow。
5. 迁移 Offer 响应式容器，再迁移传输详情的 projection/progress 展示。
6. 迁移收件箱多文件清单，保留单文件富媒体预览与既有打开/分享语义。
7. 删除旧 `file-tree`、页面私有 FileRow/SharedFileRow 和 headless-tree 依赖，运行完整静态检查、单元/组件测试与双平台人工验收。

若中途需要回滚，保留 adapters 与新组件不影响 native 数据；尚未迁移的页面可以继续使用旧 FileTree。只有在所有入口完成迁移后才删除旧组件与依赖。

## Open Questions

无阻塞问题。视觉细节（不同宽度下 grid 列数和卡片比例）在实现时按现有设计 token 与设备宽度计算，但不得改变本设计的身份、状态、虚拟化和滚动所有权契约。
