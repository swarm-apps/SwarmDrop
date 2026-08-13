## Context

三端各有一份「文件明细」：

| 端 | 位置 | 行数 | 形态 |
|---|---|---|---|
| 桌面 | `src/components/file-browser/` | 1343 | tree + grid，`previewUrl` 缩略图，`@headless-tree` 驱动树 |
| 移动 | `mobile/src/components/file-browser/` | 1506 | tree + grid，`useFileThumbnail` + 视频海报磁盘缓存 |
| Web | `transfer-detail.tsx:248` 的 `TransferFileList` | 62 | 平铺列表，无视图切换、无预览 |

桌面与移动的分歧不是版本差，是**分叉**：移动端 `size: bigint`（uniffi）、`localUri` 而非 `previewUrl`、status 多 `paused` / `cancelled` 两档；`media-type.ts` 只在移动端存在，注释却写着「file-browser 全链路唯一来源」。

Web 那份还带着一个取数缺陷：`live?.files ?? projection.files` 二选一，消费点靠 `"transferred" in file` / `"status" in file` 现场嗅探形状。用户已撞上「切会话后文件列表串了别的会话的文件」。桌面同一件事走的是 `fromTransferProjectionFiles(files, { progress, defaultStatus })`——progress 是覆盖层，单一形状。

约束：

- `packages/shared-view` 的门禁是 `lib: ["ES2022"]`（无 DOM、`types: []`）+ 禁止任何非相对路径 import。**React 组件进不去，DOM API 也进不去。**
- 三端 i18n 版本分裂：桌面 Lingui 5.9（Vite plugin + babel macro）、Web 6.6（SWC plugin）、移动 6.0（Metro transformer）。
- `docs/` 与 `mobile/` 是**独立 pnpm workspace**，各自用 `link:` 指回 `packages/*`；`docs` 还需 `transpilePackages` 才编得动 TS 源。
- Web 是静态导出（`output: "export"`），组件必须 `"use client"`，且不能引入需要服务端的东西。

## Goals / Non-Goals

**Goals:**

- 「文件明细」的 React DOM 表现层**只有一份**，桌面与 Web 共用同一份 JSX 与文案
- 建树、媒体判定、来源归一三件事在三端**只有一份**实现，移动端 RN 表现层消费同一套逻辑
- Web 端补齐 tree / grid 双视图与缩略图，行为与另两端一致
- 删掉「同一份数据两种形状」的取数方式，让那个串会话的 bug 失去落脚点
- 三端 Lingui 版本统一到 6.x

**Non-Goals:**

- 不改 Rust 侧任何事件契约（`per-file-progress`、`send-progress` 不动）
- 不统一移动端的 RN 表现层——React Native 与 React DOM 不共用 JSX，两份实现是物理必然
- 不做向后兼容：`FileBrowserItem` 直接收敛成一份，各端在 adapter 边界转换
- 不给共享包做预构建产物，沿用 shared-view 已确立的「发布 TS 源」约定
- 不在本次引入视频缩略图到 Web（移动端的视频海报管线依赖原生解码，Web 侧是另一个课题）

## Decisions

### D1 — 三层切分，React 组件包与纯逻辑包分开

```
packages/shared-view/src/file-browser/    ← L1 纯逻辑，三端共用
  tree-data.ts     路径归一 + 建树 + dataLoader 数据结构
  media-type.ts    图片 / 视频扩展名判定（从移动端整体上移）
  adapters.ts      四种来源 → FileBrowserItem
  thumbnail.ts     缩略图**契约**：该不该生成、目标规格、缓存 key
  types.ts         FileBrowserItem / Status / View / Scope

packages/file-browser/src/               ← L2 React DOM，桌面 + Web 共用
  file-browser.tsx  外壳 + 视图切换 + 计数
  file-tree-view / file-grid-view / file-card / file-row / folder-row / item-actions
  use-thumbnail.ts  缩略图 hook（取图源由 props 注入）

src/components/file-browser/             ← 删除
docs/app/app/.../TransferFileList        ← 删除
mobile/src/components/file-browser/      ← 保留 RN 表现层，逻辑三文件改 re-export
```

**为什么不合成一个包**：`shared-view` 的两道门禁（无 DOM lib、禁非相对 import）正是它能被移动端安全消费的原因。为了塞进 React 组件而放宽它们，等于把这个包的价值取消掉。两个包各守各的边界，`packages/file-browser` 依赖 `shared-view`，方向单一。

> **2026-08-06 实读两端代码后的修正**：原文说 `tree-data` 是「合并桌面 117 行与移动 148 行两份实现」，
> **不准确**。两者不是同一算法的两份写法，是**两种数据结构**：
>
> | | 桌面 | 移动 |
> |---|---|---|
> | 输出 | `{ nodes: Map, children: Map, rootChildren, dataLoader }` | `{ roots: 嵌套树, directoryIds }` + `flattenVisibleNodes` |
> | 为谁而设 | `@headless-tree` 的 `dataLoader` | FlashList 的扁平行（自带 `depth`） |
> | `normalizeRelativePath` | 单参 | 双参（带 fallbackName） |
> | `size` | `number` | `bigint` |
>
> 真正同义的是**核心算法**（按 `relativePath` 派生目录层级、目录累计 `size`/`fileCount`、
> 目录优先 + 名称 `localeCompare` 排序）。所以 L1 只收这一层，输出**中立的嵌套树**，
> 桌面在 L2 里把它投影成 headless-tree 要的 Map + dataLoader，移动继续用它的扁平化。
>
> 另外，移动端的 `src/core/file-browser-identity.ts`（`normalizeRelativePath` /
> `selectedFileId` / `sessionFileId` / `inboxFileId` / `isPathInsideDirectory`）也属于 L1——
> 它已经被抽出组件目录了，只是抽到了移动端自己的 `core/`。

**为什么 `thumbnail.ts` 的契约在 L1 而管线在 L2**：`createImageBitmap` / `OffscreenCanvas` 是 DOM API，进不了 L1。但「哪些文件该有缩略图」「缩到多大」「缓存 key 怎么算」三端必须一致，那是纯函数，正好落在 L1。

### D2 — 组件自带 `<Trans>`，文案落各端 catalog

`packages/file-browser` 的组件内部直接写 `<Trans>` 与 `useLingui()` 的 `t`。桌面与 Web 各自的 `lingui.config.ts` 把包源码加进 `include`：

```ts
// lingui.config.ts（桌面）          docs/lingui.config.ts（Web）
include: ["src", "../packages/file-browser/src"]   include: ["app/app", "../packages/file-browser/src"]
```

两端各自 extract、各自翻译，同一句话在两份 catalog 里各存一份。

**这不是重复，是既定约定**：CLAUDE.md 写死了「三端同一组 locale、**三份独立 catalog**」。给共享包单独一份 catalog 意味着各端运行时要加载第二个 i18n 实例并管理它的激活时机——为十几条文案付这个代价不划算。

**为什么不把文案做成 props**：`FileBrowser` 会多出十余个文案 prop，每个调用点都要凑齐，三端反而更容易各写各的。

**翻译入口统一为 `useLingui()` 的 `t`**，不用 `@lingui/core/macro` 的全局 `t`——共享包不该依赖某一端的全局 i18n 单例。桌面现有的 `file-browser` 已经是这个写法，这条不构成额外改造。

### D3 — 桌面 Lingui 5.9 → 6.x

共享组件里的宏由**消费端**的编译器展开，宏与运行时必须同版本。桌面是唯一落后的一端（Web 6.6 / 移动 6.0），所以升桌面。

涉及 `@lingui/core`、`@lingui/react`、`@lingui/cli`、`@lingui/format-po`、`@lingui/vite-plugin`、`@lingui/babel-plugin-lingui-macro` 六个包。宏的包路径（`@lingui/core/macro` / `@lingui/react/macro`）在 5.x 已经就位，桌面全仓都在用新路径，这一层不动。

**作为独立的第一步做**，与组件重构分开提交：它波及桌面全量文案，混在一起出问题时分不清是升级坏的还是重构坏的。

### D4 — 跨 workspace 的包必须走 `file:`，不能走 `link:`

> **2026-08-06 spike 已执行，本条据实测结果重写。** 原推断只对了一半：宏的展开确实如预期，
> 但漏掉了**运行时实例分裂**这一层——那才是真正会炸的地方。

**宏的展开（推断成立）**：桌面 Vite 的 `preserveSymlinks: false` 会把包解析成仓库内真实路径，
`@vitejs/plugin-react` 的 babel 够得着；Web 的 `transpilePackages` 登记后，SWC 的 Lingui plugin
随 Next 管线作用于包源码。两端 `lingui extract` 也都扫得到（桌面 506→508、Web 457→459）。

**运行时实例分裂（原设计未预见）**：Web 侧用 `link:` 时构建在**预渲染阶段**失败——

```
TypeError: Cannot destructure property 'i18n' of 'j(...)' as it is null.
```

不是宏没展开（`✓ Compiled successfully` 之后才炸），而是模块解析落到了不同的物理副本：

| 解析起点 | 落到 |
|---|---|
| `packages/file-browser/src`（`link:` 的真实路径） | 仓库根 `@lingui/react@5.9`、`react@19.2.4` |
| `docs/app/app` | `docs/node_modules` 的 `@lingui/react@6.6`、`react@19.2.7` |

两个副本 = 两个 `React.createContext` = 组件永远读到 `null`。

**这与 Lingui 无关，是布局问题**：`react` 同样分裂，所以**任何带 hooks 的共享组件**都会撞上
（`useState` 从错误的 dispatcher 读，直接 "Invalid hook call"）。`shared-view` 之所以从没暴露
这件事，是因为它**零运行时 import**——那不是运气，是它「零依赖」判据的直接收益。

**解法**：Web 侧改用 `file:../packages/file-browser`。pnpm 会把包装进 docs 自己的虚拟 store
（`docs/node_modules/.pnpm/@swarmdrop+file-browser@file+.../`），解析上下文随之变成 docs 的依赖树，
两边落回同一份副本。实测改完即通过：`next build` 成功，预渲染 HTML 里 `<Trans>` 与 `` t`` ``
两种宏的文案都正确输出。

**代价**：pnpm 对 `file:` 用硬链接——**改已有文件实时生效，新增 / 删除文件不同步**，
必须重跑 `cd docs && pnpm install`。日常改动是修改已有文件，不受影响；第 4 组一次性迁入
15 个文件后要记得重装一次。

**升 Lingui 6 解决不了这件事**（原以为版本对齐就够了）：根 workspace 与 docs workspace 各有
自己的 `.pnpm` 目录，同版本也是两个物理副本，React 按文件路径判定模块身份。所以 D3 的升级
是为了「宏与运行时同版本」，不是为了修分裂——分裂由 `file:` 修。

推论已写进 `packages/file-browser/README.md`：**本仓 `packages/*` 下任何有运行时 import 的包，
被独立 workspace 消费时都必须走 `file:`。**

### D5 — `FileBrowserItem` 收敛成一份

取三端并集，分歧在 adapter 边界解决：

| 字段 | 决定 | 理由 |
|---|---|---|
| `size` | `number` | 移动端的 `bigint` 是 uniffi 的产物，在移动端 adapter 里 `Number(...)` 转。文件大小不会溢出 `Number.MAX_SAFE_INTEGER` |
| 预览源 | `previewSource?: string` | 语义按端不同（桌面 asset URL / 移动 `file://` / Web OPFS relative path），由各端 adapter 填，L2 只负责交给取图源函数 |
| `status` | 并集：`idle` `waiting` `transferring` `paused` `completed` `cancelled` `error` `missing` | 移动端多的两档是真实状态，桌面缺它们是缺陷不是简化 |
| `sourceId` | `string \| number` | 桌面已是联合，移动端窄成 `string`，取宽的 |

### D6 — progress 是覆盖层，不是替代品

Web 端的取数改成与桌面同一个签名：

```ts
// 之前（二选一 + 形状嗅探）
const files = live?.files ?? projection.files;
const done = "transferred" in file ? file.transferred : file.transferredBytes;

// 之后（单一形状）
const items = fromTransferProjectionFiles(projection.files, { progress, defaultStatus });
```

`projection.files` 永远是骨架（文件身份、大小、路径都在它里面），`progress` 只覆盖在途的字节数与状态。**「终态时忽略 progress」这条判定一并收进 adapter**——它现在散在 `transferSample` 里，靠每个消费点自觉带上，而 `progress` 域从不清理，任何一处漏带就会读到陈旧快照。

**蓝本取移动端那份，不是桌面那份**（2026-08-06 实读后确定）。`mobile/src/components/file-browser/adapters.ts`
的 `fromProjection` 已经是这个形状：

```ts
const progressByFileId = new Map(progress?.files.map((f) => [f.fileId, f]) ?? []);
return projection.files.map((file) => {
  const live = progressByFileId.get(file.fileId);
  const transferred = live?.transferred ?? file.transferredBytes;  // ← 逐文件覆盖，不是整块二选一
  ...
});
```

它还带着一张完整的 `phase × terminalReason → status` 映射（`projectionFileStatus`），
覆盖 `paused` / `cancelled` 两档——那正是 D5 里要并进统一模型的两个状态。
桌面的 `fromTransferProjectionFiles` 只有 `defaultStatus` 一个粗粒度回退，表达不了这些。

### D7 — Web 缩略图管线

`crates/web` 补一条导出：

```rust
/// OPFS 文件句柄 → `File`（惰性引用，不读字节）。
pub async fn open_file(relative_path: &str) -> Result<File, JsValue>
```

`export_blob_url` 就是它 + `createObjectURL` 的组合（`crates/web/src/opfs.rs:190`），重构成两层即可，下载路径不变。

**为什么不直接复用 `export_blob_url`**：它返回 URL 字符串，而 `createImageBitmap` 只接受 `Blob` / `ImageBitmapSource`，拿到 URL 还得 `fetch` 一次绕回 Blob——多一次拷贝，且中间那个 object URL 必须记得 revoke。

管线（`packages/file-browser/src/use-thumbnail.ts`，取图源由 props 注入）：

```
File → createImageBitmap → OffscreenCanvas(≤320px 长边)
     → convertToBlob({ type: "image/webp", quality: 0.7 })
     → createObjectURL → LRU(64) 缓存
```

7.6 MB 的原图产出约 30 KB。**解码峰值仍是单文件大小**（图片必须完整解码），所以配尺寸门槛（>20 MB 直接给类型图标）与串行队列，避免一屏同时解码。缓存条目被挤出时 `revokeObjectURL`。

非 secure origin 下 OPFS 不可用（`docs/app/app/_lib/secure-context.ts` 已有判定），取图源返回 `undefined`，卡片降级到类型图标。

### D8 — 视图偏好三端同构

桌面已有 `preferences-store.fileBrowserViews`（按 `FileBrowserScope` 分别记忆），移动端已有同结构。Web 端在 `_lib/preferences-store.ts`（localStorage）补一份。`FileBrowserScope` 枚举从 L1 出，三端不各写各的。

## Risks / Trade-offs

- **Lingui 6 的破坏面未逐条核实** → 独立第一步，跑完桌面 `pnpm test` + `pnpm build` + 三个 locale 的 `i18n:extract` 再往下走；这一步单独一个 commit，坏了能直接回退。
- ~~**link 包里的宏展开可能不通**~~ → **已排除**（2026-08-06 spike）。真正的坑是运行时实例分裂，
  由 `file:` 协议解决，见重写后的 D4。残留风险是 `file:` 的硬链接语义：新增文件不同步，
  第 4 组批量迁入后必须重跑 `docs` 的 `pnpm install`。
- **`@headless-tree` 在 Next 静态导出下未验证** → 它是纯客户端库，组件带 `"use client"` 即可；Web 侧先只接传输详情一处，通了再铺开到收件箱与发送。
- **一屏解码大图导致卡顿** → 尺寸门槛 + 串行队列 + 只对进入视口的项触发。
- **删桌面组件波及 6 个消费点与 4 个测试文件** → 测试随组件迁进新包，消费点只改 import 路径（组件 API 不变）。
- **两份 catalog 存同一句话** → 接受。这是三端 catalog 独立这条既定约定的必然结果，代价是翻译时多译十余条。

## Migration Plan

1. **spike**：`packages/file-browser` 放一个只含 `<Trans>` 的组件，桌面与 Web 各接一次，验证宏展开 + extract + 运行时渲染（D4）
2. **Lingui 6**：桌面六个包升级，重新 extract / compile，全量回归（D3）
3. **L1 下沉**：`tree-data` / `media-type` / `adapters` / `thumbnail` 契约进 `shared-view`；移动端三文件改 re-export，移动端 `pnpm typecheck` 绿
4. **L2 建包**：桌面 `src/components/file-browser/` 整体迁入 `packages/file-browser`，桌面 6 个消费点改 import，测试跟着迁；桌面这一步**行为零变化**
5. **Web 接入**：传输详情先接（含 D6 的 adapter 改造），再铺收件箱与发送；缩略图管线与 `open_file` 导出同期落地
6. 收尾：删 `TransferFileList`，更新 `dev-notes/knowledge/` 与 `packages/*/README.md` 的归属判据

每步独立可验证，1–4 步之间桌面与移动的行为都不变。

## Open Questions

- Lingui 6 是否需要全量重新翻译（预期 `.po` 格式不变、msgid 不变，但要在第 2 步实证）
- Web 端 `projection.files` 的 `relativePath` 是否与 OPFS 落盘路径逐字一致——缩略图取图源依赖这条映射，不一致就要在 adapter 里补一层
- Web 的发送面板选文件阶段拿到的是浏览器 `File` 对象（不在 OPFS 里），取图源要走另一条分支，第 5 步再定
