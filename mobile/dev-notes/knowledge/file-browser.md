# 文件浏览

## 统一模型与依赖方向

移动端的发送选择、接收 Offer、传输投影和收件箱文件先通过 adapter 转换为统一的
`FileBrowserItem[]`。tree 与 grid 只负责展示同一份叶子集合，页面通过显式 actions 注入删除、打开、分享和重试能力。

**正确做法**：

- 选择文件使用 `sourceId`，Offer/投影使用 session + fileId，收件箱使用 item + fileId 生成稳定 ID。
- `relativePath` 只用于层级和展示；目录删除必须按 segment 边界判断，不能用裸 `startsWith`。
- 路径归一化只统一分隔符和空路径段，不 trim 文件名中的合法空格。
- identity 与选择集合操作放在 `src/core/file-browser-identity.ts`，store 不反向依赖 UI component。
- `MobileTransferProjection` 是持久事实源，实时 progress 只按 fileId 覆盖 transferred/status。

**不要做**：

- 不要按 relativePath 去重叶子文件；不同来源可能有相同路径和文件名。
- 不要让 FileBrowser 根据 route、scope 或 URI 猜测业务能力。

**相关文件**：`src/core/file-browser-identity.ts`、`src/components/file-browser/`

## 虚拟列表与滚动所有权

普通页面使用 FlashList，bottom sheet 中使用 `BottomSheetFlatList`。页面只能有一个同方向主滚动容器，固定操作栏必须留在虚拟列表外并处理 Safe Area。

**正确做法**：

- tree 先把展开节点拍平成 rows 再虚拟化；grid 通过 `numColumns` 渲染叶子。
- Offer 的来源、策略、保存位置和拒绝/接收操作固定，中间文件集合独立滚动。
- `AppBottomSheet virtualized` 只提供固定高度容器，让 children 自己拥有 BottomSheet 虚拟列表。
- 禁止关闭的 Offer 同时禁用下拉和遮罩点击，避免 sheet 消失但队列状态仍保留。

**不要做**：

- 不要在 ScrollView 中嵌套同方向 FlashList/FlatList。
- 不要使用已废弃的 `BottomSheetFlashList`。
- 不要用 `map` 渲染可能达到 10,000 项的文件集合。

**相关文件**：`src/components/file-browser/file-tree-view.tsx`、`src/components/file-browser/file-grid-view.tsx`、`src/components/transfer-offer-host.tsx`

## 预览权限与 WebDriver fixture

缩略图权限边界留在 adapter：只有调用方确认可访问时才提供 `previewUri`，失败后回退文件类型图标。Offer 接收前不提供预览 URI。

文件浏览的确定性模型和大集合场景沿用 `e2e/webdriver`。开发构建通过应用内的 dev-only 路由进入 fixture；不要依赖自定义 scheme deep link，因为 `expo-share-intent` 会参与 URL 分发，可能使 Router 导航不稳定。

**相关文件**：`src/app/e2e/file-browser.tsx`、`e2e/webdriver/test/specs/file-browser.e2e.ts`
