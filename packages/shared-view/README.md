# @swarmdrop/shared-view

三端（桌面 `src/` · Web `docs/app/app` · 移动 `mobile/src`）共享的**纯视图逻辑**：
设备显示投影与格式化。零运行时依赖、零平台依赖。

由 openspec change `web-ux-alignment` 引入。它是「三层分离」里的 L1——
L2 是 `DESIGN.md` 的跨端交互契约，L3 是各端各写的表现层。

## 归属判据：什么该进这里

三条**全部**满足才进：

1. **纯函数、零平台依赖** —— 不碰 DOM / RN / Node / IPC / 文件系统 / i18n 运行时。
2. **至少两端在用**（或本次重写后会有两端在用）—— 只有一端用的东西留在那一端，
   放进来只是把局部逻辑摊成全局 API，换不来任何共享。
3. **输出跨端一致** —— 各端有意呈现不同的东西**不进来**，因为收进来就必须改掉其中一端的
   渲染输出。已知按这条判出去的：
   - `formatUptime`：桌面「2 小时 15 分钟」/ 移动「2h 15m」，两种本地化文案。
   - `formatRelativeTime`：三端各带各的 i18n（桌面硬编码中文、移动返回 `<Trans>` 节点）。
   - `defaultReceivePolicy` 等信任策略默认值：引用平台保存位置与 uniffi 枚举。

判据 3 的一个推论：**函数不该把 UI 占位文案烤进返回值**。`formatTransferRate` 因此在
「算不出速率」时返回 `null`，由调用方给自己的 `—` / `等待数据` / `Unknown`——
否则这个函数就同时是格式化器和一句待翻译的文案，两端一定会想要不同的那句。

## 类型边界：结构化入参，不 import 任何一端的 bindings

三端的 `Device` 由三条 codegen 产出（tauri-specta / wasm-bindgen / uniffi）。本包若 import
其中任一，就同时（a）把该端的构建产物变成另外两端的依赖，（b）在 wasm 产物未构建时让桌面
类型检查失败。

所以本包的入参一律**只声明用到的字段**（`DeviceNameSource`、`IdentifiedDevice` …），
三端各自的类型都能结构化赋值，零 `as` 断言。

移动端的字段名差异（`latencyMs` vs `latency`）由**调用点**适配，本包不设适配层——
适配层会把「哪一端叫什么」这个知识带进本该平台中立的包里。

## 门禁：`pnpm check:shared-view`

两条合起来覆盖「零平台依赖」：

| 门 | 挡住什么 | 挡不住什么 |
|---|---|---|
| `tsconfig.json` 的 `lib: ["ES2022"]`（无 DOM、`types: []`） | `document.` / `window.` / `process.` | `import { useState } from "react"` |
| `scripts/check-shared-view-imports.mjs` | 任何非相对路径 import（测试文件可 import `vitest`） | —— |

第二道不能省：包虽然嵌在仓库根之下、`package.json` 里一个 dependency 都没有，但 tsc 的模块
解析会一路向上走到**仓库根的 `node_modules`** 并解析成功。pnpm 的 isolated 链接兜不住这件事，
实测确认过。

第一道只在**对本包自身**跑 tsc 时成立——三端各自 typecheck 用的是各自的 lib，跟进来的源文件
不受它约束。所以 `pnpm check:shared-view` 必须留在提交前清单里。

## 三端怎么接

发布的是 **TS 源**，不预构建 `dist/`（openspec `web-ux-alignment` 的 design D2）。
三端各付一行构建配置：

| 端 | 声明 | 构建配置 |
|---|---|---|
| 桌面（Vite） | 根 workspace member，`"workspace:*"` | 无 |
| Web（Next / turbopack） | `"link:../packages/shared-view"` | `transpilePackages` + `turbopack.root` 放到仓库根 |
| 移动（Metro） | `"link:../packages/shared-view"` | `watchFolders` + `resolver.nodeModulesPaths` |

**Web 那条 `turbopack.root` 是必需的**：turbopack 的 root 是文件系统边界，落在它之外的文件
进不了模块图。锁在 `docs/` 时 `pnpm typecheck` 全绿（tsc 沿 symlink 解析得到）而
`next build` 报 `Module not found` —— 只有构建那一步会红。

## 消费方式：只从包根导入

```ts
import { deviceDisplayName, formatFileSize } from "@swarmdrop/shared-view";
```

不要深链 `@swarmdrop/shared-view/src/device/name`。子模块划分是本包的内部事，
改动它不该惊动三个消费者。

## 各端的 facade 与直接 import

各端原有的同名模块按一条规则处理：

- **还留有平台特有逻辑** → 保留为 facade，把共享部分原样 re-export。调用方仍从一处拿全套。
  （如 `src/lib/device-name.ts` 还有改名编排与事件订阅。）
- **会变成 100% 转发** → 删掉，调用点直接从本包 import。纯转发模块是死间接层。

## 测试

测试与源码同目录（`*.test.ts`），由仓库根的 `pnpm test`（vitest）收录——
根 `vitest.config.ts` 没有排除 `packages/`，默认 include 就能扫到，不需要单独的配置。
