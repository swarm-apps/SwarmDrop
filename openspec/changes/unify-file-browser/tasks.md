> ## 进度：代码全部完成（2026-08-06）
>
> **未完的只剩人工验证**：4.7（桌面四处的树/网格回归）、6.1（Web 复现串会话那个 bug 已消失）、
> 7.1–7.4（缩略图的内存 / 门槛 / 损坏图 / 非 secure origin 四条运行时行为）。
> 这七条都要跑起来看，机器门禁替不了。
>
> ### 验收记录
>
> | 门 | 结果 |
> |---|---|
> | 桌面 `pnpm test` | 29 文件 195 用例 |
> | 桌面 `pnpm build` | 通过 |
> | 桌面 `i18n:extract` | 508 条，Missing 0 |
> | `check:shared-view` / `check:zustand-access` | 绿 |
> | `docs` `pnpm typecheck` / `pnpm build` | 通过（静态导出 26 页） |
> | Web `i18n:extract` | 433 条，Missing 0 |
> | `cargo check --workspace --all-targets` | 通过 |
> | `check-wasm.sh`（含 `--clippy`） | 通过 |
> | `test-wasm.sh` | 23 用例通过 |
> | `mobile` `pnpm typecheck` / `lint` / `i18n:extract` | 全绿（655 条，Missing 0） |
>
> ### 环境：这块 USB 外置盘会把构建卡死
>
> 与本次改动无关，但下次碰到别重复排查：
>
> - 卡住的进程处在 `U`（不可中断 I/O 等待），`kill -9` 打不动——那是内核态，用户态程序造不出来。
> - **与磁盘剩余空间无关**（12G 时卡过，60G 时也卡过），**与 Lingui 6 无关**
>   （`git stash` 回 5.9 重装后同样卡，已排除）。
> - 规律：重启后一切正常（vitest 1–3 秒），**一旦跑过 `pnpm install` / `cargo clean` 这类
>   批量小文件写入，之后 vitest / vite build 就会卡进 `U`**；此时单文件读写仍是毫秒级。
> - 目前唯一可靠的恢复手段是重启。

## 1. Spike：验证共享包里的 Lingui 宏能被两端展开 ✅ 已完成（2026-08-06）

这一组不过，design 的 D2 需要推翻重议，后面全部作废——所以它排第一。

**结论：通过，但发现了 design 未预见的坑**——宏的展开如预期，真正会炸的是**运行时实例分裂**
（`link:` 让包解析到仓库根的 `@lingui/react@5.9` + `react@19.2.4`，而 docs 用 6.6 + 19.2.7，
两个 React context 永远读到 `null`）。改用 `file:` 协议后通过。D4 已按实测重写。

- [x] 1.1 建 `packages/file-browser` 骨架：`package.json`（`@swarmdrop/file-browser`，`main`/`types` 指 `src/index.ts`，同 shared-view 的「发布 TS 源」约定）、`tsconfig.json`、`README.md`
- [x] 1.2 桌面 `package.json` 加 `workspace:*`；`docs/package.json` 用 **`file:../packages/file-browser`**（**不是 `link:`**——理由见 README 的「依赖协议必须是 file:」）
- [x] 1.3 包内放一个只含 `<Trans>` 与 `useLingui()` 的探针组件，桌面与 Web 各挂一处
- [x] 1.4 桌面：vitest（与 `vite.config.ts` 同一套 babel macro 配置）渲染断言两种宏形态均展开；`pnpm build` 通过
- [x] 1.5 Web：`docs/next.config.mjs` 的 `transpilePackages` 加 `@swarmdrop/file-browser`；`pnpm build` 通过，预渲染 HTML 里两条文案都正确输出
- [x] 1.6 两端 `lingui.config.ts` 的 `include` 加包源码路径（**桌面不带 `../`**，其 rootDir 就是仓库根；docs 那份要带），各跑一次 extract 确认提取到
- [x] 1.7 spike 结论写进 `packages/file-browser/README.md`；探针与临时接线已清理，两端 catalog 用 `--clean` 归零（顺带清掉 42 条历史废弃条目）

## 2. 桌面 Lingui 5.9 → 6.x

独立提交，与组件重构分开——它波及桌面全量文案。

> **2026-08-06 进度**：代码改动完成，**构建类验证被磁盘阻塞**（见下方「环境」）。
> 已确认的破坏面比预想小：没有用到 v6 移除的 Intl wrapper（`i18n.date()` / `i18n.number()`），
> Node v24 满足 v6 的 ≥22.19 要求，宏路径（`@lingui/core/macro` / `@lingui/react/macro`）
> 在 v5 就已就位，全仓在用新路径。

- [x] 2.1 升级六个包：`@lingui/core`、`@lingui/react`、`@lingui/cli`、`@lingui/format-po`、`@lingui/vite-plugin`、`@lingui/babel-plugin-lingui-macro` → 均 6.6.0（与 Web 端同版本）
- [x] 2.2 按官方 migration guide 过 5→6 破坏面：**`src/` 下无需修改**（三条破坏面都不适用，理由见上）
- [x] 2.3 `pnpm i18n:extract` + compile 三个 locale，diff `.po` 确认 msgid 未大规模变动 ← **待磁盘恢复**
- [x] 2.4 `pnpm test` + `pnpm build`（tsc + vite）冒烟 ← **待磁盘恢复**；`tsc --noEmit` 已单独通过
- [x] 2.5 确认三端 `@lingui/core` 与 `@lingui/react` 同为 6.x（桌面 6.6.0 / Web 6.6.0 / 移动 6.0.1）
- [x] 2.6 **修 peer 不匹配**：`@lingui/vite-plugin@6.6.0` 要求 `@babel/core: ^7.29.0 || ^8.0.0`，实际解析到 7.28.6（`@vitejs/plugin-react` 的传递依赖）。pnpm 只发了一句 "Issues with peer dependencies found" 就放过了。用 `pnpm.overrides` 提升

## 3. L1：纯逻辑下沉 `packages/shared-view/src/file-browser/`

> **2026-08-06 进度**：3.1–3.8 的代码与单测已写完，`pnpm check:shared-view` 两道门禁通过
> （import 纯度 + 无 DOM 的 tsc）。单测执行被磁盘阻塞。3.9（移动端 re-export）**刻意未动**——
> 它要把移动端的 `size: bigint` 整体切成 `number`，波及该端所有消费点，不该在验证不可用时写。

- [x] 3.1 `types.ts`：`FileBrowserItem`（`size: number`、`previewSource?`、`sourceId: string | number`）、`FileBrowserStatus`（八档并集）、`FileBrowserView`、`FileBrowserScope`、树节点类型
- [x] 3.2 `media-type.ts`：从 `mobile/src/components/file-browser/media-type.ts` 整体上移（它已是「唯一来源」，只是唯一性此前只在移动端成立）
- [x] 3.3 `tree-data.ts`：只下沉**核心算法**（按 relativePath 派生目录层级 + 目录累计 size/fileCount + 目录优先排序），输出**中立的嵌套树**。两端现有实现是两种数据结构而非两份写法（见 design D1 的修正表），所以桌面在 L2 里投影成 `@headless-tree` 的 Map + dataLoader，移动继续用自己的 `flattenVisibleNodes`
- [x] 3.3b `identity.ts`：把移动端 `src/core/file-browser-identity.ts` 的 `normalizeRelativePath`（双参版为准）/ `selectedFileId` / `sessionFileId` / `inboxFileId` / `isPathInsideDirectory` 一并上移；桌面的 `getParentPath` 也归这里
- [x] 3.4 `adapters.ts`：四种来源归一——发送枚举、入站 offer、传输投影 + 进度、收件箱条目。**以移动端那份为蓝本**（它已实现 progress 逐文件覆盖 + 完整 phase×terminalReason→status 映射）。入参按 shared-view 既有约定**只声明用到的字段**，不 import 任何一端的 bindings
- [x] 3.5 `fromProjectionFiles(sessionId, files, { phase, terminalReason, progress })`：`projection.files` 为骨架、progress 按 `fileId` 覆盖，**终态忽略 progress 的判定收在函数内部**；status 映射移植移动端的 `projectionFileStatus`。签名比原计划多两项——`sessionId` 用于构造展示 ID（offer 与投影同一 fileId 必须得到同一个 ID），`phase`/`terminalReason` 取代桌面那个粗粒度的 `defaultStatus`（它表达不了 paused / cancelled）
- [x] 3.6 `thumbnail.ts`：缩略图契约——`shouldGenerateThumbnail(item)`、目标长边常量、尺寸门槛常量、缓存 key 生成。纯函数，不碰 DOM
- [x] 3.7 单测覆盖建树、媒体判定、四个 adapter，尤其是「终态忽略 progress」与「progress 只覆盖不新增条目」两条
- [x] 3.8 `pnpm check:shared-view` 通过（两道门禁自动覆盖新目录）
- [x] 3.9 移动端 `tree-data.ts` / `media-type.ts` / `adapters.ts` / `types.ts` / `core/file-browser-identity.ts` 改为 re-export + `bigint → number` 边界转换；`mobile/` 下 `pnpm typecheck` / `pnpm lint` / `i18n:extract` 全绿。<br>三处**行为变化**（都是刻意的）：<br>① `localUri` → `previewSource`（三端统一字段名；语义仍按端不同，所以 `useFileThumbnail` 里的 `file://` 前缀判定保留）；<br>② 入站 offer 的状态从 `waiting` 改成 `idle`，与另两端对齐；<br>③ 顺带修掉一个**既有 i18n 漏洞**：`fileStatusText` 把 `t` 当形参收，babel 宏只认词法作用域里的 `useLingui()` 解构，所以那八条状态文案**从来没进过 catalog**，英文界面上一直显示中文。改成模块级 `msg` 描述符 Record + 调用点 `t(...)`（三端一致的写法）。<br>另外 `sourceId` 在共享模型里是 `string \| number`（收件箱那一路用行主键），发送侧两个调用点补了一次 `String(...)` 收窄
- [x] 3.10 更新 `packages/shared-view/README.md` 的归属判据，说明「缩略图契约进来、管线不进来」的分界

## 4. L2：建组件包，桌面整体迁入

> **2026-08-06 完成**（4.3 与 4.7 除外）。验收：`tsc --noEmit` 0、`pnpm test` 28 文件 190 用例、
> `pnpm build` 4.26s、`check:shared-view` / `check:zustand-access` 绿、`i18n:extract` Missing 0。
>
> **原计划说的「行为零变化」不完全成立**，两处实质变更：
>
> 1. **逐文件状态改由 `phase + terminalReason` 推断**（原先是调用方算一个粗粒度的
>    `defaultStatus` 灌进去）。收益是能表达 `paused` / `cancelled` 两档，且 active 会话在
>    没有实时事件时也能按 `transferredBytes` 判出 `transferring`（断点续传后正是这种情形）。
> 2. **入站 offer 的状态从 `waiting` 修回 `idle`**——迁移中一度跟了移动端的 `waiting`，
>    那是回归：offer 是「要不要收」的决策依据，挂等待图标会暗示传输已经开始。已加测试钉死。
>
> 另有一处非行为差异：展示 ID 的前缀统一成了 L1 的 `source:` / `session:` / `inbox:`
> （原桌面是 `send:` / `offer:` / `transfer:` / `inbox:`）。它只作 React key 与树节点 id。

- [x] 4.1 `src/components/file-browser/` 的 9 个组件文件迁入 `packages/file-browser/src/`，逻辑部分改为消费 L1
- [x] 4.1b `headless-tree-adapter.ts`（L2 内）：把 L1 的嵌套树投影成 `@headless-tree` 要的 `{ getItem, getChildren }` + `rootItemId: "root"`。`FileTreeView` 现在直接吃 `treeData.dataLoader`（`file-tree-view.tsx:24`），而 L1 刻意只给中立嵌套树（design D1 的修正），这一层就是差额
- [x] 4.2 4 个测试文件一并迁入，在新包内跑通
- [x] 4.3 `use-thumbnail.ts`：缩略图 hook，取图源由 props 注入，LRU(64) + 视口触发（IntersectionObserver，200px 预取边距）+ 并发闸门（`THUMBNAIL_CONCURRENCY`）+ 同 key 去重 + 淘汰即 revoke。**卸载不 revoke**——URL 在共享缓存里，别的卡片可能正用着。三条护栏由 `use-thumbnail.test.tsx` 钉住（去重 / 并发峰值 / 淘汰数），它们都没有反馈回路：漏了不报错，只会顶崩标签页或悄悄泄漏位图<br>**取图源的类型从 `Promise<string \| null>` 改成 `Promise<Blob \| null>`**：管线第一步 `createImageBitmap` 只吃 `Blob`，给 URL 还得 `fetch` 一次绕回来。桌面**不传** resolver，卡片直接用它那个已经能渲染的 asset URL（`FileCard` 里两条路径二选一、不做回退——Web 的 `previewSource` 是 OPFS 相对路径，塞进 `<img src>` 只会拿到 404）
- [x] 4.4 桌面取图源接线：收在 `src/lib/file-browser-adapters.ts` 的 `previewSourceOf`（`convertFileSrc` + `isImageFile` 判定），行为与原 `previewUrl` 一致；发送侧仍不给取图源（不为预览扩大 asset scope）
- [x] 4.5 桌面 6 个消费点改 import：`components/transfer/session-panel.tsx`、`components/transfer/transfer-offer-dialog.tsx`、`routes/_app/inbox/index.lazy.tsx`、`routes/_app/send/index.lazy.tsx`、`routes/_app/send/share-target.lazy.tsx`、`routes/_app/send/-use-file-selection.ts`、`stores/preferences-store.ts`
- [x] 4.6 删除 `src/components/file-browser/`
- [ ] 4.7 **需手动跑一次**桌面回归：发送选文件、传输详情、收件箱详情、入站 offer 对话框四处的树形与网格都正常，视图偏好仍按 scope 记忆
- [x] 4.8 `pnpm test` + `pnpm build` 绿

## 5. Web 接入与取数根治

> **2026-08-06 完成非 Rust 部分**（5.1 / 5.2 / 5.6 留给缩略图管线一起做）。验收：
> `docs` 的 `tsc --noEmit` 0、`pnpm build` 静态导出 26 页全过、`check:zustand-access` 绿、
> 两端 `i18n:extract` Missing 0（桌面 508、Web 433）。
>
> 计划外多做的两件，与「三端同一件事只有一份实现」同一条理由：
>
> 1. **入站 offer 对话框（`transfer-offer-host.tsx`）也接入了**。它原是「列前 5 个 + 还有 N 个」，
>    而那一屏正是用户决定收不收的唯一依据——桌面 `TransferOfferDialog` 早就是 `FileBrowser`。
>    收件箱那份「待处理请求」列表**刻意保持** 5 条预览：它是多条 offer 的索引，不是决策面
>    （理由写在 `incoming-offers-panel.tsx` 的常量注释里）。
> 2. **L2 的动作集合补了 `onDownload` + `pendingIds`**。浏览器没有「打开文件 / 在文件夹中
>    显示」，桌面没有「下载」——动作集合按**并集**定义、靠「不传就不渲染」按端裁剪，
>    而不是在组件里 `if (platform)`。顺带给行/卡的悬停动作条加了 `pointer-coarse:opacity-100`：
>    触摸屏没有 hover，藏起来等于没有，而 Web 端是移动优先的。
>
> 另修掉一个第 4 组引入的回归：`fromInboxFiles` 丢了 `sourceId`（收件箱的「打开 / 在文件夹中
> 显示」按它拿行主键），桌面上会变成 `Number(undefined)` = NaN。已补测试钉死。

- [x] 5.0 先在 `docs/` 重跑一次 `pnpm install`。**结论比原计划更严厉**：`file:` 的硬链接在实践中根本不同步——编辑器写临时文件再 rename，inode 一换链接就断。**改了 L2 就必须重装**，症状会伪装成 `Module not found`（新增文件）或「某属性不存在于类型上」（修改文件）。已改写 `packages/file-browser/README.md` 与 `dev-notes/knowledge/toolchain.md`
- [x] 5.1 `crates/web`：`opfs.rs` 拆出 `open_file(relative_path) -> File`（惰性句柄，不读字节），`export_blob_url` 改为它 + `createObjectURL`；下载路径行为不变。**5s 超时兜底跟着下移到 `open_file`**，并给它补了与 `export_blob_url` 同款的「未就绪路径必须快速失败」回归测试——网格视图会对一屏十几个条目同时取图，少了这条超时，一个未就绪的路径就能挂住一个并发槽不放
- [x] 5.2 `crates/web/src/node.rs` 导出 `open_file`；`./scripts/check-wasm.sh`（含 `--clippy`）与 `./scripts/test-wasm.sh`（23 用例）通过，`pnpm build:wasm` 已重生成产物
- [x] 5.3 `docs/app/app/_lib/preferences-store.ts` 加 `fileBrowserViews`（按 scope，localStorage 持久化）。默认值与桌面逐项一致（inbox 网格、send/transfer 树形），逐 scope 校验非法值退回默认
- [x] 5.4 传输详情接入 `FileBrowser`：`live?.files ?? projection.files` 与 `"transferred" in file` 形状嗅探都已删除，改走 `itemsFromProjection`
- [x] 5.5 删除 `TransferFileList`（连同只服务于它的 `TransferFileRow` 与 `FILE_LIST_LIMIT`）。**`transferSample` 的 terminal 判定没有第二个消费点**——它是单一函数，三个渲染点共用；现在它只管会话级字节与百分比，逐文件那一路的同名判定已收进 L1 的 `fromProjectionFiles`
- [x] 5.6 Web 取图源接线（`_lib/thumbnail-source.ts`）：收件箱走 `opfsThumbnailSource`（`previewSource` = `file.relativePath`，与 `download_url` 同一个字段）；**非 secure origin 提前判掉**而不是让 `open_file` 报错——那条路径每张图都要付一次 5s 超时的等待。<br>**发送侧另走一条**（design 的 Open Question 3 的答案）：待发文件的字节只活在内存里的 `File` 句柄上，没有任何路径指得到它，所以 `previewSource` 存的是自增序号，由 `createPendingFileThumbnailSource` 按它回查
- [x] 5.7 收件箱（`inbox-views.tsx`）接入 `FileBrowser`；下载走新的 `onDownload` + `pendingIds`，失败逐条报并带上文件名（`WebErrorCard` 补了 `title` 覆盖位）
- [x] 5.8 发送面板（`send-panel.tsx`）接入；移除按来源键（自增序号）而不是路径——同一个文件可以被选两次，按路径删会把两行一起带走。目录整体移除走 `isPathInsideDirectory`
- [x] 5.9 `pnpm check:zustand-access` 通过（规则 B 覆盖 `docs/app/app`）
- [x] 5.10 `docs` 下 `pnpm build` 通过（静态导出 26 页）

## 6. 验证串会话那个 bug 已消失

- [ ] 6.1 复现路径：Web 端准备三条以上不同文件的会话，在列表里反复切换，确认详情侧文件清单恒等于该会话 `projection.files`，条目数不随切换次数增长
- [x] 6.2 回归测试已随 L1 单测落地（`adapters.test.ts` 的「反复用不同会话的进度调用，输出互不污染」+「progress 多出来的 fileId 不会新增行」）
- [x] 6.3 同上（`adapters.test.ts` 的「终态会话不读残留的进度快照」+「判定在函数内部，调用方不传 progress 也是同一结果」）

## 7. 缩略图端到端验证

- [ ] 7.1 Web 网格视图滚动一批数 MB 图片，确认常驻内存与原图大小无关（缓存里只有缩放产物）
- [ ] 7.2 超过尺寸门槛的图片不触发解码，直接显示类型图标
- [ ] 7.3 损坏图片解码失败时该条目回落图标，同列表其他条目照常
- [ ] 7.4 非 secure origin 下网格视图全部图标，无报错
- [x] 7.5 **契约无第二份常量**（`grep THUMBNAIL_*` 只命中 `packages/shared-view/src/file-browser/thumbnail.ts` 及其消费点）。<br>**但要说清适用范围**：`shouldGenerateThumbnail` / `thumbnailTargetSize` / `THUMBNAIL_*` 目前只被桌面与 Web 消费。移动端的管线是 expo-image 原生解码（原生侧自己做降采样与调度），既不需要 20 MB 门槛也不需要 JS 侧并发闸门，且它**还有视频海报**——本次不引入 Web 视频缩略图，所以那一路本就在共享契约的覆盖范围之外。强行套用会让移动端丢掉视频海报

## 8. 收尾

- [x] 8.1 `packages/file-browser/README.md` 写清归属判据（L1 / L2 / 各端），另补两节：动作集合按**并集**定义靠「不传就不渲染」按端裁剪；缩略图的契约/管线/取图源三层分工与「桌面不传 resolver」的理由
- [x] 8.2 更新 `CLAUDE.md`：`packages/` 一节与 Key File Locations 补 `file-browser`（含 `file:` 协议那条硬约束），Tech Stack 的 i18n 行改为三端同为 Lingui 6，桌面 i18n 段补 `include` 含共享包
- [x] 8.3 知识库四处：`theme-and-styling.md` 加「共享包的 `<Trans>` 靠 `include` 提取，两份配置 rootDir 不同」与「`t` 不能当形参」；`web-app-frontend.md` 加「取数走 adapter 不做形状嗅探」与「改了 L2 必须在 docs 重装」；`toolchain.md` 改写硬链接那条（实践中根本不同步）；`mobile/dev-notes/knowledge/file-browser.md` 加上移后的边界与同一条宏陷阱
- [x] 8.4 机器门禁全绿（见顶部验收表）→ `/simplify` 四路审查（复用 / 简化 / 效率 / 分层）→ 见下

## 9. `/simplify` 四路审查的落地（2026-08-06）

四个并行 agent 各审一角，去重后动了 20 余处。**最深的三条**：

### 9.1 `previewSource` 一个字段被当三种东西用

L2 的 `FileCard` 靠 **`thumbnailSource` 这个 prop 在不在**来判断自己拿到的是「可直接渲染的
URL」还是「要解析的句柄」——把「调用方有没有传取图源函数」当成「我跑在哪一端」的代理变量，
而类型系统一个字都表达不出那条不变量，只能靠 README 一段散文拴住。

**修法**：`FileBrowserItem` 拆成 `previewUrl`（桌面的 asset URL，直接渲染）与
`previewSource`（Web / 移动，需 resolver + 管线）。`FileCard` 改成
`item.previewUrl ?? thumbnailUrl`，两条路**由字段本身区分**。它们本来就是两件事，
不是同一件事的三个名字。

### 9.2 传输中每 tick 全量重建整棵树，而三个行组件根本没有 memo

`items` 每秒都是新数组 → `buildFileBrowserTree`（含每个目录一次 sort）→ `toHeadlessTreeData`
→ headless-tree 全量 `rebuildTree()` → 所有可见行重渲染。而**树的拓扑在一次会话里恒定**，
每秒变的只有 `status` / `progress`。

**修法**：树的 memo 依赖换成拓扑签名（id + 路径 + 大小），行渲染改从 `liveItems` 这张 Map
查当前值；`FileRow` / `FileCard` / `FolderRow` 补 `memo` + 自定义比较器（默认浅比较对每秒
重建的 item 永远判不等，加了等于没加）；四个调用点的 `actions` 与 `pendingIds` 提进 `useMemo`
（`useKeyedAsyncAction` 也改成返回稳定引用），否则 memo 在第一层就被打穿。
顺带修掉挂载时 `rebuildTree()` 跑两遍，与 `localeCompare(a, b, {...})` 改复用 `Intl.Collator`。

### 9.3 取字节占着解码槽

并发闸门存在的理由是「解码是 CPU 与内存双重密集」，但它同时圈住了取字节——Web 那条是 OPFS
遍历、最坏 5s 超时。于是**一个取不到的文件就冻结四分之一管线最长 5 秒**，而它一个字节都
没在解码。改成 I/O 在闸门外、只有 `renderThumbnail` 在闸门内。同时补了**负缓存**：
虚拟列表滚出去再滚回来会重挂载 → 重试 → 又占一个槽，一批取不到的条目上下滚一次就能打满管线。

### 其余（按类）

**该上提没上提的**：视图偏好的默认值 + 归一三端各一份（这次差点抄成第三份），已上提到 L1 的
`view-preference.ts`；桌面发送页手写的「精确路径或目录前缀」匹配换成共享的
`isPathInsideDirectory`（那份手写版还漏了路径归一，Windows 反斜杠对不上）。

**枚举纪律半途而废**：`SessionTerminalReason` 把 `fatal_error` 改名成 `failed`，代价是桌面与
Web 各留一段 16 行、逐字相同的改名 switch——而它们吃的本来就是同一个 wire 字符串联合。
已回退成与 wire 同名，两段 switch 整体消失。`ProgressFileInput.status` 从 `string` 收成真枚举，
连带删掉两个在三端 codegen 里根本不存在的死分支（`"failed"` / `"error"`，从移动端旧实现搬来的）。

**类型收窄成本外包**：`sourceId` 从 `string | number` 收成 `string`——那个 union 让六个调用点
各写一次 `Number(...)` / `String(...)`，而这类转换没有反馈回路（本次已经收过一次账：
`Number(undefined) = NaN`）。

**重复**：桌面 `src/lib/file-icon.ts` 整份删除（它自带的扩展名表与 `media-type` 长期不一致，
有 `ico` 却缺 `heic`/`avif`/`tiff`），收件箱改用共享的 `getFileIconStyle`；移动端
`file-icon.ts` 私藏的三张表换成 `fileCategory()`；L1 的 `progressPercent` 换成同包的
`calcPercent`；三份薄适配里的恒等 `.map()` 改直传。

**死代码 / 过宽的面**：L2 的公开面从 22 个符号收到 5 个（`FileBrowser` + 四个类型 +
`getFileIconStyle`）——其余外部一个消费者都没有，导出去等于承诺「树库不换、卡片可单独复用」；
删掉零调用的 `nodeItem`、只有测试传过的 `availableViews`、名不副实的三个 testId prop、
没人传的 `maxEdge` 形参、两个空操作的 `key`、一条永远到不了的 `: null` 分支、
以及 5 处与组件默认值相同的 `title={<Trans>文件</Trans>}`。

**Rust**：`opfs_root()` 加 `thread_local` 缓存——根句柄在文档生命周期内恒有效，而此前
`export_blob_url` 只在点下载时偶尔调，现在缩略图会成批调 `open_file`，那笔异步往返
（外加一个超时定时器）就成了常态开销。

### 明确跳过的

- **`Progress` 与 Web 的 `ProgressBar` 合并**：形态相同但视觉不同（`bg-primary` vs
  `--brand-solid`，后者还带 `motion-reduce:transition-none`）。要收得先让 L2 补上 motion-reduce
  纪律并把填充色做成可覆盖的——那是另一件事，会改 Web 的观感。
- **`useFileBrowserView` 便利 hook**：能把 9 个调用点各 3 行压成 1 行，但要每端加一个文件，
  收益不抵这次改动量。
- **发送侧缩略图按内容去重**（`name#size#lastModified` 当 key）：同一个文件选两次会解码两遍，
  但改 key 会动身份语义，收益边际。
- **`FileBrowserTree.totalSize` / `totalCount` 是否该删**：两位审查者结论相反（一个说没人用
  该删、一个说表头该用它）。网格视图不建树，为表头去建一棵树更贵——维持现状，表头那次
  `reduce` 已经 memo 掉了。
