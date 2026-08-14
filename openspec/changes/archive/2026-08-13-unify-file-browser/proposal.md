## Why

「文件明细」在三端有三份实现：桌面 `src/components/file-browser/`（1343 行，树形 + 网格双视图 + 缩略图）、移动 `mobile/src/components/file-browser/`（1506 行，同形态 + 缩略图管线），Web 端只有 `transfer-detail.tsx` 里 62 行的平铺列表——无视图切换、无预览。桌面改了 Web 不会跟。

更要紧的是 Web 那份的取数形状。它写作 `live?.files ?? projection.files`（`docs/app/app/_components/transfer-detail.tsx:105`）——进度事件与投影**二选一**，于是消费点必须现场嗅探形状（`"transferred" in file`、`"status" in file`）。同一份数据两种形状、判别散在渲染层，用户已经撞上「切换会话后文件列表串了别的会话的文件」。桌面那份早就把这件事收进了 adapter：`fromTransferProjectionFiles(files, { progress, defaultStatus })`——progress 是**覆盖层**而不是替代品，单一形状，没有嗅探。

三份实现里，只有 React DOM 那两端（桌面 / Web）本可以真正共用同一套 JSX。移动端是 React Native，表现层必然独立，但纯逻辑（建树、媒体判定、来源归一）三端完全同义——移动端的 `media-type.ts` 注释里已经写着「file-browser 全链路唯一来源」，只是这个「唯一」目前只在移动端成立。

## What Changes

- 新增 `packages/file-browser`：React DOM 文件浏览器共享包（tree / grid 双视图、视图切换、文件卡片、行、文件夹行、item actions），**桌面与 Web 共用同一份 JSX 与文案**
- 新增 `packages/shared-view/src/file-browser/`：三端共享的纯逻辑——`tree-data`（路径归一 + 建树）、`media-type`（图片 / 视频扩展名判定）、`adapters`（四种来源归一为 `FileBrowserItem`）
- **BREAKING**：桌面 Lingui `5.9` → `6.x`。共享组件自带 `<Trans>`，三端必须同一个宏运行时；桌面是唯一落后的一端（Web 6.6 / 移动 6.0）。涉及 `@lingui/core`、`@lingui/react`、`@lingui/cli`、`@lingui/format-po`、`@lingui/vite-plugin`、`@lingui/babel-plugin-lingui-macro`
- **BREAKING**：共享组件的翻译入口统一为 `useLingui()` 的 `t`，不再用 `@lingui/core/macro` 的全局 `t`——包不该依赖某一端的全局 i18n 单例
- Web 端接入完整文件浏览器：传输详情、收件箱、发送三处的文件区都换成 `FileBrowser`，补齐树形 + 网格双视图与视图偏好持久化
- 新增 Web 端缩略图管线：OPFS 读文件 → `createImageBitmap` → `OffscreenCanvas` 缩到 320px → WebP → LRU 缓存，与移动端 `use-file-thumbnail.ts` 同构。原图不进内存常驻
- **根治取数形状**：Web 端删掉 `live?.files ?? projection.files` 与形状嗅探，改走 `fromTransferProjectionFiles(projection.files, { progress, defaultStatus })`；顺带清理「`progress` 域从不清理、靠各消费点自己带 terminal 判定」的陈旧快照隐患
- **删除** `src/components/file-browser/`（15 个文件）与 `docs/app/app/_components/transfer-detail.tsx` 里的 `TransferFileList`
- 移动端保留 RN 表现层，`tree-data` / `media-type` / `adapters` 三个文件改为消费 `@swarmdrop/shared-view`

不做向后兼容：`FileBrowserItem` 的字段在三端本就有分歧（移动端 `size: bigint`、`localUri`；桌面 `size: number`、`previewUrl`），本次直接收敛成一份，各端在 adapter 边界做类型转换。

## Capabilities

### New Capabilities

- `file-browser`: 三端统一的文件浏览器——`FileBrowserItem` 统一模型、四种数据来源（发送枚举 / 入站 offer / 传输投影 + 进度 / 收件箱条目）的归一契约、树形与网格双视图行为、视图偏好持久化、包边界与三端归属判据
- `file-thumbnail`: 网格视图缩略图——生成判定（类型 / 尺寸门槛）、目标规格与缓存契约、生命周期（视口触发、卸载释放），以及三端各自的取图源实现（桌面 `convertFileSrc` / 移动 `localUri` / Web OPFS）

### Modified Capabilities

无。Rust 侧的 `per-file-progress`、`send-progress` 事件契约不变，本次只改前端的消费方式；`transfer-detail-page` 的页面级 requirement 也不涉及文件明细组件形态。

## Impact

**新增**
- `packages/file-browser/`（React DOM 共享包）——依赖 `react`、`lucide-react`、`@lingui/react` 6、`@headless-tree/core`、`@headless-tree/react`
- `packages/shared-view/src/file-browser/`——受该包既有门禁约束（`lib: ["ES2022"]` 无 DOM、禁止非相对路径 import），故只收纯函数，缩略图管线不进去

**修改**
- 桌面 6 个消费点改 import：`src/components/transfer/session-panel.tsx`、`transfer-offer-dialog.tsx`、`src/routes/_app/inbox/index.lazy.tsx`、`src/routes/_app/send/{index,share-target}.lazy.tsx`、`-use-file-selection.ts`、`src/stores/preferences-store.ts`
- 桌面全量 i18n：Lingui 6 升级后 `src/locales/{zh,zh-TW,en}/messages.po` 需重新提取编译
- Web：`docs/package.json` 新增 `@swarmdrop/file-browser`（link:）与 headless-tree 两个依赖；`transfer-detail.tsx`、`inbox-views.tsx`、`send-panel.tsx` 接入；`_lib/preferences-store.ts` 加视图偏好
- Web wasm：缩略图需要 `Blob` 而非 URL 字符串，`crates/web` 的 OPFS 导出面可能要补一条（design 定）
- 移动：`mobile/src/components/file-browser/{tree-data,media-type,adapters}.ts` 改为消费 shared-view；8 个消费点不动

**删除**
- `src/components/file-browser/`（含 4 个测试文件，测试迁到新包）
- `docs/app/app/_components/transfer-detail.tsx` 的 `TransferFileList`

**门禁**
- `pnpm check:shared-view` 自动覆盖新增目录（两道门都是包级）
- 新包需接入根 workspace 的 typecheck；`docs` 与 `mobile` 各自 `link:` 引用
- Lingui 升级须过桌面 `pnpm test` 与 `pnpm build`（tsc + vite）
