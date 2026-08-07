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

## 展示模型与归一逻辑已上移到 `@swarmdrop/shared-view`（2026-08-06）

`types.ts` / `tree-data.ts` / `media-type.ts` / `adapters.ts` / `core/file-browser-identity.ts`
现在都只是**转发层**。本端曾是三端里做得最对的一份（progress 覆盖层 + 完整
`phase × terminalReason → status` 映射），上移时就是以它为蓝本；桌面与 Web 那两份被它取代。

改这一块前先读 `packages/shared-view/src/file-browser/` 与 `packages/file-browser/README.md`
的归属判据：**判定与算法在共享包，枚举映射与平台类型转换留本端**。

**正确做法**：

- `FileBrowserItem.size` 是 **`number`**，不是 `bigint`。uniffi 的 `u64` 在 `adapters.ts`
  一层 `Number(...)` 转完（文件大小碰不到 9 PB）。表现层不该再出现 `0n` 或 `BigInt` 混算。
- 缩略图取图源字段叫 **`previewSource`**（此前 `localUri`），语义是「**要解析**的取图源」：
  本端是 `file://`、Web 是 OPFS 相对路径。所以 `useFileThumbnail` 里那句
  `startsWith("file://")` 必须留着——不能假定它能直接喂 `<Image>`。
  展示模型上另有一个 `previewUrl`（「**可直接渲染**的 URL」，桌面的 asset URL 走那条），
  **本端不用它**。两个字段是两件事，别合并——合了的话表现层只能靠别处的线索去猜自己拿到
  的是哪一种。
- `sourceId` 恒为 **`string`**（收件箱那一路是行主键，adapter 里 `String(...)` 过一道）。
  它曾是 `string | number`，代价是每个消费点各写一次收窄——而这类转换没有反馈回路，
  漏了就是运行时的 `NaN`。
- `MobileFileProgress.status` 是裸 `string`（uniffi 不给枚举），共享包要的是真枚举。
  adapter 里的 `toSharedFileStatus` 负责收窄，**认不出来返回 `undefined`** 让 L1 回落到按
  阶段推断——比硬塞一个它不认识的值安全。
- 入站 offer 的逐文件状态是 **`idle`** 而不是 `waiting`：它还没进传输队列，挂等待图标会暗示
  传输已经开始，而此刻用户连接受都没点。`idle` 的文案因此也不能写「已选择」——那一档同时
  服务发送侧和 offer，而 offer 里的文件是**对方**选的。

**不要做**：

- 不要在本端重新实现建树、路径归一或 status 映射——那些改动必须回到共享包，否则三端立刻分叉。
- 不要把 uniffi 的 `Mobile*` 类型泄进共享包：那会让桌面与 Web 的构建依赖上本端的 codegen 产物。

**相关文件**：`src/components/file-browser/adapters.ts`、
`packages/shared-view/src/file-browser/`（仓库根）

## 翻译宏只在词法作用域里展开——`t` 不能当形参传

`fileStatusText(status, t)` 这种写法**看起来**在国际化，实际上一条都没被 extract：babel 宏认的是
词法作用域里的 `const { t } = useLingui()`，`t` 一旦变成形参，`` t`已完成` `` 就只是个普通模板
字符串。catalog 里没有对应 msgid，运行时回落到 msgid 本身，于是英文界面上原样显示中文。

**这个失败模式完全静默**：typecheck 过、lint 过、`i18n:extract` 也不报错（它根本没看见这些串）。
唯一的察觉方式是切到英文后肉眼看。

**正确做法**：模块级 `Record<K, MessageDescriptor>` 存 `` msg`…` ``，组件里 `t(TABLE[key])` 展开。
三端同一个写法（Web 的 `docs/app/app/_lib/view-types.ts`、桌面的 `transfer-labels.ts`）。

**不要做**：不要把 `t` 或 `i18n` 当参数往纯函数里传来「共享翻译」。

**相关文件**：`src/components/file-browser/file-row.tsx`

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
