# @swarmdrop/file-browser

桌面（`src/`）与 Web（`docs/app/app`）共用的**文件浏览器表现层**（React DOM）：树形 / 网格双
视图、文件卡片与行、缩略图管线。

由 openspec change `unify-file-browser` 引入。它是「三层分离」里的 **L2**——L1 是
`@swarmdrop/shared-view` 里的纯逻辑（建树 / 媒体判定 / 来源归一 / 缩略图契约），
L3 是各端自己的接线。

移动端**不消费本包**：React Native 与 React DOM 不共用 JSX，`mobile/src/components/file-browser/`
是独立的 RN 表现层，它与本包共用的是 L1。

## 归属判据：什么该进这里

- **进 L1（`shared-view`）**：纯函数、零平台依赖、三端共用。建树、路径归一、扩展名判定、
  数据来源归一、缩略图的**判定与规格**。
- **进 L2（本包）**：React DOM 组件与只在 DOM 环境成立的 hook。缩略图的**管线**在这里
  （`createImageBitmap` / `OffscreenCanvas` 是 DOM API，进不了 L1）。
- **留各端**：取图源（桌面走 Tauri 资源协议、Web 走 OPFS、移动走 `file://`）、
  业务动作的实现、视图偏好的持久化后端。

一条推论：**本包的组件不认识任何一端的存储形态**。取图源与 actions 一律由 props 注入，
组件不从 `localPath` 自己拼 URL——那条边界在桌面时代就立过（见
`dev-notes/knowledge/file-browser.md`「操作与安全边界」），迁包后继续成立。

### 动作集合是两端的并集，不是交集

`FileBrowserActions` 里同时有只有桌面给得出的（`onOpen` / `onReveal`——浏览器没有「在文件夹中
显示」）和只有 Web 需要的（`onDownload`——桌面接收即落盘，没有第二次「取回」这个动作）。
按端裁剪靠**「不传就不渲染」**，不靠组件里的 `if (platform)`：后者会让本包开始认识它跑在哪，
而那正是三端统一想消除的东西。

### 缩略图：契约在 L1，管线在 L2，取图源在各端

- **判定与规格**（该不该生成、缩到多大、缓存 key）在 L1 的 `thumbnail.ts`。
- **管线**在本包的 `use-thumbnail.ts`：`Blob → createImageBitmap → canvas(≤320px) → webp →
  objectURL`，外加 LRU(64)、并发闸门、视口触发、淘汰即 revoke、失败负缓存。
- **取图源**（`ThumbnailResolver`）由调用方注入，收 `previewSource` 字符串、返回 **`Blob`**
  ——管线第一步 `createImageBitmap` 只吃 `Blob`，给 URL 还得 `fetch` 一次绕回来。

**两条取图路径由字段区分，不由 prop 的有无反推**：

| 字段 | 谁给 | 怎么用 |
|---|---|---|
| `previewUrl` | 桌面（`convertFileSrc` 的 asset URL） | 直接 `<img src>`，不经管线 |
| `previewSource` | Web（OPFS 路径）、移动（`file://`） | 经 resolver 取字节 → 管线 |

这两个曾是同一个字段，于是 `FileCard` 只能靠「调用方有没有传 `thumbnailSource`」反推自己
拿到的是哪一种——那等于让组件去猜它跑在哪一端，而类型系统一个字都表达不出来。它们本来就是
两件事，不是同一件事的两个名字。

**并发闸门只圈解码，不圈取字节**：闸门存在的理由是「解码是 CPU 与内存双重密集」，而取字节是
纯 I/O（Web 那条最坏 5s 超时）。圈进去的话，一个取不到的文件就冻结四分之一管线最长 5 秒，
而它一个字节都没在解码。

## ⚠️ 依赖协议必须是 `file:`，不能是 `link:`

**这是本包与 `shared-view` 最关键的差别，接线时第一个要注意的事。**

| 消费端 | 协议 | 原因 |
|---|---|---|
| 桌面（根 workspace） | `workspace:*` | 与本包共用同一份根 `node_modules`，天然不分裂 |
| Web（`docs/`，独立 workspace） | **`file:../packages/file-browser`** | 见下 |

Web 侧若沿用 `shared-view` 那样的 `link:`，构建会在**预渲染阶段**炸掉：

```
TypeError: Cannot destructure property 'i18n' of 'j(...)' as it is null.
  at ../packages/file-browser/src/....tsx
```

原因不是宏没展开（宏展开得好好的，`✓ Compiled successfully`），而是**运行时实例分裂**：

- `link:` 是纯软链。Node / Turbopack 解析真实路径后，从 `packages/file-browser/src/` 一路
  **向上**找 `node_modules`，撞到的是**仓库根**那份——桌面的 `@lingui/react@5.9`、`react@19.2.4`。
- 而 docs 应用树用的是 `docs/node_modules` 里的 `@lingui/react@6.6`、`react@19.2.7`。
- 两个物理副本 = 两个 `React.createContext` = 组件读到的 context 永远是 `null`。

`file:` 让 pnpm 把本包装进 **docs 自己的虚拟 store**
（`docs/node_modules/.pnpm/@swarmdrop+file-browser@file+..+packages+file-browser/`），
解析上下文随之变成 docs 的依赖树，两边落到同一份副本。

**这个问题不限于 Lingui。** `react` 同样分裂，所以**任何带 hooks 的共享组件**都会撞上
（`useState` 会从错误的 dispatcher 读，直接 "Invalid hook call"）。
`shared-view` 从没暴露过这件事，是因为它**零运行时 import**——那不是运气，是它 README 里
「零运行时依赖」那条判据的直接收益。

推论：**本仓 `packages/*` 下任何有运行时 import 的包，被独立 workspace（`docs/` / `mobile/`）
消费时都必须走 `file:`。**

### `file:` 的代价：**改完必须重跑 `cd docs && pnpm install`**

pnpm 对 `file:` 目录依赖用**硬链接**。硬链接理论上共享 inode，改内容两边都能看到——
**但几乎所有编辑器与工具都不原地写**，它们写临时文件再 `rename()` 覆盖，于是新文件是新
inode，硬链接当场断开，`docs/node_modules/@swarmdrop/file-browser/` 里留着的还是旧内容。

所以实际规律是这一条，不要再指望「改已有文件会自动同步」：

| 操作 | 是否同步 |
|---|---|
| 新增 / 删除文件 | ❌ |
| 修改已有文件 | ❌（编辑器一 rename 就断链） |

**症状会伪装成别的问题**：新增文件表现为 Next 的 `Module not found`；修改已有文件表现为
`tsc` 报「某某属性不存在于类型上」——你明明刚加了那个属性。看到这两种里的任何一种，
先 `cd docs && pnpm install`（几秒钟），再去怀疑代码。

## Lingui：组件自带宏，文案落各端 catalog

组件内联 `<Trans>` 与 `useLingui()` 的 `t`，**不通过 props 接收 UI 文案**，也不依赖任一端的
全局 i18n 单例（所以用 `useLingui()` 而非 `@lingui/core/macro` 的全局 `t`）。

两端各自的 `lingui.config.ts` 把本包源码纳入 `include`：

```ts
// 仓库根 lingui.config.ts（rootDir 就是仓库根，**没有** `../`）
include: ["src", "packages/file-browser/src"]

// docs/lingui.config.ts（rootDir 是 docs/，所以要 `../`）
include: ["app/app", "../packages/file-browser/src"]
```

同一句话在两端的 `.po` 里各存一份。这不是重复劳动，是「三端 catalog 独立」这条既定约定
（见 `CLAUDE.md`）的必然结果——给本包单独一份 catalog 意味着各端运行时要加载并管理第二个
i18n 实例，为十几条文案不值得。

**改了本包的任何文案，两端都要 `pnpm i18n:extract` 并把 Missing 补回 0**，
Web 侧尤其致命：catalog 里查不到的 id 在生产构建里会显示成六位随机字符串
（见 `dev-notes/knowledge/toolchain.md`）。

## 已验证的接线（2026-08-06，spike 结论）

| 验证项 | 桌面 | Web |
|---|---|---|
| 宏被编译器展开 | ✅ Vite + `@lingui/babel-plugin-lingui-macro` | ✅ Next SWC + `@lingui/swc-plugin`（需 `transpilePackages`） |
| `<Trans>` 与 `` t`` `` 两种形态 | ✅ vitest 渲染断言 | ✅ 预渲染 HTML 中两条文案都正确输出 |
| `lingui extract` 扫得到 | ✅ 506 → 508 | ✅ 457 → 459 |
| 运行时 context 不分裂 | ✅ 共用根 `node_modules` | ✅ **仅在 `file:` 下成立**（`link:` 会 null） |
| 生产构建 | ✅ `pnpm build` | ✅ `pnpm build`（静态导出 26 页） |
