## Context

### 当前状态

Web 端的**内核已与桌面平权，前端停在 demo**。`crates/web/src/node.rs:591` 的 `paired_devices()`
调用的是与桌面 `list_devices` 同一条 `DeviceManager::get_devices(DeviceFilter::Paired)`，返回的
`Device` 与 `src/lib/bindings.ts:313` 字段字节级同构（同源 `crates/host/src/device.rs:436`）。
Web 前端只渲染其中 3 个字段。

三端的实现规模差距：

| | 桌面 | 移动 | Web |
|---|---|---|---|
| 设备卡片 | `device-card.tsx` 459 行 | `device-card.tsx` 238 行（row/card 双变体） | `device-list.tsx` 149 行（整个列表） |
| 设备页编排 | `devices/index.lazy.tsx` 604 行 | 主屏 + `device/[peerId]` 详情 | 两块平铺面板 |
| 发送入口 | 点设备卡片 | 点设备卡片 | 发送页 `<select>` 下拉 |
| UI 底座 | shadcn/ui（24 个组件） | 自建 RN 组件 + NativeWind | 手写原生元素 |
| i18n | Lingui 5.9（Vite plugin + babel macro） | Lingui 6.0.1（Metro transformer） | 无（硬编码中文） |

### 既有约束（不可破坏）

- **运行时单例只挂 layout**：`WebNodeBootstrap` 同时负责 spawn 节点、事件消费、状态轮询、relay 意图。
  下放到 page 会变成每路由一份。
- **静态导出三限制**：无动态路由段、内部导航走 `next/link`、`useSearchParams()` 必须套 `<Suspense>`。
- **zustand 门禁**：`pnpm check:zustand-access` 的规则 B 覆盖 `docs/app/app`。
- **导航单一事实源**：`docs/app/app/_lib/nav.ts`，跨页链接不得手拼字面量。
- **`docs/` 是独立 pnpm workspace**（自带 lockfile 与 `pnpm-workspace.yaml`），`turbopack.root` 显式
  锁在 `docs/`。
- **子路径部署**：`basePath` 经 `PAGES_BASE_PATH` 注入，裸字符串路径要手动拼前缀。

### 已定的四个方向（本设计的前提）

1. 三层分离：纯逻辑共享包 + `DESIGN.md` 契约 + 各端表现层各写
2. `docs` 接入 shadcn/ui + fumadocs token 映射
3. 移动优先，≥920px 渐进为主从双栏
4. Lingui 本次一起进 Next

## Goals / Non-Goals

**Goals:**

- Web 端设备呈现补齐到与桌面/移动同等的信息密度——数据已在，只差渲染
- 三端发送心智统一为「设备优先」
- 三端重复的纯视图逻辑收口到一处，且机制上阻止再长副本
- 交互契约成文，成为后续所有设备相关 UI 的评审依据
- Web 端具备 dialog / dropdown / sheet 等原语，使信任策略、别名分组这类操作成为可能
- Web 端文案可国际化

**Non-Goals:**

- 不改桌面端与移动端的**渲染输出**。契约是对它们现状的追认
- 不共享带样式的 React 组件
- 不做兼容层、不做双写、不做渐进迁移
- 不换文档区的皮，不改营销页
- ~~不改任何 Rust crate~~ —— 实施中经用户确认放开。信任策略编辑在 wasm 侧没有写入路径
  （`WebNode` 的 41 个方法里没有 `update_paired_device_policy`），不补它那条需求做不了。
  补的是 `crates/web` 的一个薄壳导出，业务逻辑仍在 core 的 `set_receive_policy`——
  与桌面命令同一条路径，不是给 Web 单开一份。其余 crate 仍是零改动。
- 不做 Web 端的发送侧续传（浏览器限制，属既有结论）

## Decisions

### D1 — 表现层刻意不共享，只共享逻辑与契约

**决定**：`packages/shared-view`（纯函数）+ `DESIGN.md` 契约（规格）+ 三端各自的表现层实现。

**为什么不共享 React 组件**（`packages/ui` 方案）：

1. **移动端物理上共享不到**。RN 用 `View` / `Pressable` / `Text`，不是 DOM。共享 DOM 组件后
   仍是「桌面+Web 一份，移动一份」——从三份变两份，却要为此付跨构建系统的全部代价。
2. **桌面组件的假设不可移植**。`device-card.tsx` 写死了 `min-h-[132px]`、`hover:` 态、
   `glass-card` / `backdrop-filter`、`group-hover:scale-105`、`DropdownMenu` 悬浮定位。这些在
   375px 触摸设备上是负资产：hover 不存在、132px 固定高在两列网格里过高、`backdrop-filter`
   在移动 GPU 上是实打实的开销。
3. **两套构建系统的 CSS 处理不同**。桌面是 Vite + Tailwind v4 + `src/index.css` 的 `glass-*`
   utility；Web 是 Next(turbopack) + Tailwind v4 + fumadocs preset。共享带样式组件要么把
   `index.css` 整个搬过去（连同玻璃拟态与 WebGL 背景的依赖），要么在包里内联样式——两条路
   都比各写一份贵。

**为什么契约放 `DESIGN.md` 而不是抽象成代码**：契约要约束的是**三种不同技术栈的三份实现**，
代码抽象只能覆盖其中两份。写成规格则三份都受约束，且评审时可逐条对照。这与本仓既有做法一致
（`DESIGN.md` 已经在承担「920 断点三端同一个数」「窄屏空态分工」这类跨端约定）。

**放弃的方案**：
- `packages/ui` 共享表现层 —— 见上
- 完全不共享 —— `device-name` 已经三份、`device-organization` 两份且行为已经开始分叉
  （桌面 77 行 vs 移动 150 行），继续放任只会让第四份、第五份出现

### D2 — 共享包发布 TS 源，三端各加一行转译配置

**决定**：`packages/shared-view` 的 `package.json` 指向 `src/index.ts`，不预构建。三端各自转译：

| 端 | 引用方式 | 构建配置 |
|---|---|---|
| 桌面（Vite） | 根 workspace member，`workspace:*` | 零配置（Vite 直接吃 TS） |
| Web（Next/turbopack） | `link:../packages/shared-view` | `transpilePackages` 加一项 |
| 移动（Metro） | `link:../packages/shared-view` | `watchFolders` 加一项 |

**为什么不预构建 `dist/`**：纯函数包的 build 是秒级，但预构建引入「改了源忘了 build」的常驻风险，
且 `docs/` 与 `mobile/` 是独立 workspace，pnpm 对 `link:` 依赖不执行 lifecycle 脚本——`prepare`
钩子不会自动跑，必须在三端的 dev/build 脚本里各前置一次，比三行构建配置更脏。

**为什么不把 `dist/` 提交入库**（`docs/packages/swarmdrop-web` 的先例）：wasm 产物入库是因为
构建需要 Rust 工具链、CI 装它很贵。TS 包没有这个理由，入库只换来每次改动的 diff 噪音与
「产物与源不同步」这一类新的失败模式。

**回退路径**：若 turbopack 的 `root` 锁定或 Metro 的 symlink 解析在实践中扛不住（见 R1/R2），
退到预构建 `dist/` + 三端 dev/build 脚本前置 build。届时改动局限在包内与三处 `package.json` 脚本。

### D3 — 共享包用结构化入参，不 import 任何一端的 bindings

**决定**：共享函数的入参声明只列用到的字段，例如：

```ts
// 示意：声明最小结构，而非 import 三端任一 Device
export interface DeviceNameSource {
  peerId: string;
  deviceName?: string | null;
  hostname?: string | null;
}
```

**为什么**：三端的 `Device` 由三个 codegen 产出（tauri-specta / wasm-bindgen / uniffi）。共享包若
import 其中任一，就同时（a）把该端的构建产物变成另外两端的依赖，（b）在 wasm 产物未构建时让桌面
类型检查失败。结构化类型让三者都能直接赋值，且**字段增删时是编译错误而不是运行时惊喜**。

移动端的 `MobileDevice` 有字段名差异（`latencyMs` vs `latency`）——这类差异由**调用点**处理，
共享包不设适配层。适配层会把「哪一端叫什么」这个知识带进本该平台中立的包里。

### D4 — token 映射层：`@theme inline` 引用 fumadocs 变量，缺口自给

**决定**：在 `docs/app/global.css` 增加一层映射，不新建第三套 token：

```css
/* 示意 */
@theme inline {
  --color-background: var(--color-fd-background);
  --color-foreground: var(--color-fd-foreground);
  --color-card: var(--color-fd-card);
  --color-border: var(--color-fd-border);
  --color-muted-foreground: var(--color-fd-muted-foreground);
  /* primary 走品牌色，不跟 fumadocs 的 primary */
  --color-primary: var(--brand-solid);
  --color-primary-foreground: var(--brand-ink);
}
```

fumadocs 未覆盖的 shadcn token（`destructive` / `input` / `chart-*` / `sidebar-*`）SHALL 在同一层
自给值，明暗各一。品牌色对齐桌面 `src/index.css` 的 oklch 值（当前 Web 是另一组 hex，两者不同）。

**为什么不给应用区自建独立 token 层**：应用区与文档区在同一个 Next 应用、同一份 CSS、同一个明暗
模式开关下。自建等于把明暗模式实现两遍，且切换时会出现两区不同步的瞬态。

**风险控制**：映射层只**新增** `@theme inline` 声明，不改写任何 `--color-fd-*` 的值——文档区读的
是原变量，读不到我们的别名（见 R4 的验证方式）。

### D5 — Lingui 接入 Next：三级阶梯，先试最优

**决定**：Web 端跟随移动端的 **Lingui 6.x**（桌面仍是 5.9，两条线已分叉，本次不统一）。
接入路径按阶梯尝试，先到先用：

1. **`@lingui/swc-plugin`**（最优）—— Next 原生 SWC 管线，零额外转译开销。风险是 swc plugin 的
   ABI 与 Next 16 的 `swc_core` 版本绑定。
2. **turbopack rule + `babel-loader`**（回退 A）—— 只对 `app/app/**` 挂 `@lingui/babel-plugin-lingui-macro`。
   多一层转译，但作用域受限、影响可控。
3. **不用 macro，走显式 id**（回退 B）—— `<Trans id="…">` + `i18n._()`。零编译器插件，代价是
   人工维护 msgid、且提取要靠 `@lingui/cli` 的显式模式。

**为什么必须先做阶梯 spike**：阶梯 1 能否走通决定了后续所有组件的写法（macro vs 显式 id），
写完 19 个组件再发现不通就要全量返工。因此它是 tasks 里的**第一个**任务，且是其余 i18n 工作的
前置。

> **spike 结论（已执行）：阶梯 1 通过，回退 A/B 不需要。**
> `@lingui/swc-plugin@6.6.0` 与 Next 16.2.6 的 `swc_core` ABI 兼容——宏编译、`lingui extract`
> 提取（JSX 元素正确变成 `<0>`/`<1>` 占位）、`next build` 静态导出三件事全绿。
>
> 附带定死的两个实现细节：
>
> 1. **源 locale 的目录静态 import 并在模块加载时同步激活**（`_lib/i18n.ts`）。预渲染发生在
>    构建期，那一刻不能 await；不同步激活则预渲染出来的 HTML 是空壳。另两个 locale 按需
>    动态 import，且**显式列成三条**而非拼模板字符串——后者会让打包器生成 context 模块。
> 2. **catalog 的 `.ts` 是产物，不入库**；`.po` 才是事实源。`lingui compile --typescript`
>    挂在 `postinstall` 与 `build` 两处，保证 IDE 与 CI 都拿得到。

**locale 策略**：静态导出下不按 locale 预生成路由（会让路由数 ×3 且与 `basePath` 叠加出更多子路径）。
locale 在客户端选择，持久化到 `localStorage`，首访读 `navigator.languages`。

### D6 — Web 设备页：卡片网格 + 配对入口，发送是卡片主动作

**决定**：`/app/devices` 从「两块平铺面板」改为：

```
PageHeader
├── 设备卡片网格（自适应列数：<640 单列 · 640-919 两列 · ≥920 三列）
│   └── 每卡满足 device-presentation-contract 的全部信息位
├── 空态（无已配对设备时，教学文案指向配对）
└── 配对入口（生成邀请 / 消费邀请，折叠为次级区块）
```

发送成为**卡片主动作**：整卡可点 = 发送（在线时）；`/app/send?peerId=` 降级为直达落点。
`send-panel.tsx` 的 `<select>` 保留作纠错手段，但不再是主路径入口。

**新增能力**（数据已在，只差 UI）：信任策略编辑（`receivePolicy` / `trustLevel`）、别名与分组。
这两项需要 dialog / dropdown 原语，正是接 shadcn 的直接收益。

**为什么不照搬桌面 604 行的编排**：桌面那页还含「附近未配对设备」与节点启停 sheet——前者依赖
mDNS 局域网发现（浏览器没有），后者在 Web 端由 `WebNodeBootstrap` 自动接管。照搬会引入两个
Web 上不成立的区块。

### D7 — 主从布局 Web 自建，但断点与桌面同一个数

**决定**：收件箱与传输页改主从布局；Web 自己实现 shell（`docs/app/app/_components/master-detail.tsx`），
**不共享**桌面的 `MasterDetailShell` 组件，但 `(min-width: 920px)` 这个数与桌面
`MASTER_DETAIL_QUERY` 一致，并在两侧注释互指。

**为什么断点要同一个数**：Windows 常见的 125% 缩放下，1200 物理像素只有 960 CSS 宽——正好落在
920 与 1024 之间。用 `lg:`(1024) 会让同一台机器上桌面版分栏、Web 版堆叠。这条理由在
`CLAUDE.md` 已有记载，本次把它扩展到 Web。

**选中态承载**：继续用 `?session=` / `?item=` query param（静态导出不能用动态段，既有约束）。

### D8 — 不做兼容，直接替换

**决定**：以下全部直接换掉，不留旧路径、不双写：

- `docs/lib/cn.ts`：`export { twMerge as cn }` → `clsx + twMerge` 组合（shadcn 需要条件类名，
  当前实现传对象会产出 `[object Object]`）
- `docs/app/app/_lib/{device-name,format}.ts`：删除本地实现，改 re-export 共享包
- `src/lib/{device-name,device-organization,format}.ts` 与移动端等价物：同上
- `docs/app/app/**` 的硬编码中文：全量改 i18n 宏

**依据**：Web 端无真实用户（CLAUDE.md 对 Web schema 变更的既有结论：直接换，不写迁移/回填/双写）。
桌面与移动只改 import 路径，行为不变——由三端现有测试与共享包合并后的单测钉住。

### D9 — 契约在 `DESIGN.md` 的落点与形态

**决定**：`DESIGN.md` 第 5 节（组件规格）新增三小节：

1. **Device Card Contract** —— 信息位清单（8 项）+ 各端允许的形态差异边界
2. **Send Entry Contract** —— 发送必须从设备进入；发送页选择器只是落点
3. **Cross-platform UI Review Checklist** —— 新增设备相关 UI 时的逐条对照表

同时在既有的 Don't-list 补一条：**不得以「布局太挤」为由省略契约要求的信息位**。

## Risks / Trade-offs

**[R1] turbopack `root` 锁在 `docs/`，`link:` 依赖指向上级目录可能解析失败**
→ 缓解：这是 tasks 中共享包接入的第一个验证点，在只搬一个函数时就跑通 `next dev` + `next build`
再继续搬其余。失败则按 D2 的回退路径改预构建 `dist/`，改动局限在包内与三处脚本。

**[R2] Metro 对 workspace 外 symlink 的解析**
→ 缓解：`watchFolders` + `resolver.nodeModulesPaths` 是 RN 0.72+ 的标准做法，`mobile` 已是
RN 0.85 / Expo 56。与 R1 同批验证：搬第一个函数就跑 `pnpm typecheck` + 真机启动一次。
失败同样退到预构建。

**[R3] `@lingui/swc-plugin` 与 Next 16 的 `swc_core` ABI 不匹配**
→ 缓解：D5 的三级阶梯，且 spike 排在所有 i18n 工作之前。三级全不通的概率极低——回退 B 只依赖
Lingui 运行时，不依赖任何编译器。

**[R4] token 映射层污染文档区**
→ 缓解：映射只新增别名、不改写 `--color-fd-*` 的值。验证方式：接入后对文档区首页、任一文档页、
搜索面板各截图一次，明暗两种模式对比接入前。

**[R5] Lingui 三端版本分叉（桌面 5.9 / 移动 6.0.1 / Web 6.x）**
→ 接受。三端 catalog 本就不共享（桌面 `src/locales/`、移动 `mobile/src/locales/`、Web 新建），
版本差异不产生运行时耦合。统一版本是独立议题，不进本次范围。

**[R6] Web 端 19 个组件全量重写，既有约束可能在过程中被破坏**
→ 缓解：三条硬约束各配一道机器门禁——`pnpm check:zustand-access`（selector 规则）、
`next build`（静态导出三限制）、`pnpm typecheck`。它们在每个组件重写后都要过一次，
而不是攒到最后。运行时单例约束无机器门禁，靠 `WebNodeBootstrap` 文件顶部的既有注释 + 评审。

**[R7] 桌面/移动改 import 引入静默回归**
→ 缓解：先把三端现有测试合并进共享包并**全部跑绿**，再改三端 import。合并时若发现桌面 77 行与
移动 150 行的 `device-organization` 行为不一致，该差异要显式收敛并记录取舍——这正是收口的价值
所在，不能靠「哪个先写就用哪个」蒙过去。

**[R8] 「移动优先」会让桌面浏览器的宽屏体验退化**
→ 接受并主动管理：920 断点上的渐进增强是必答项而非可选项，收件箱/传输/设备网格三处都必须在
≥920 有明确的宽屏形态，不能停在「单栏拉宽」。这写进了 `web-app-shell` 的验收场景。

**[R9] 契约写进 `DESIGN.md` 后无机器门禁，可能随时间腐化**
→ 接受。这是 D1 的固有代价——跨三种技术栈的约束只能靠评审。缓解是把 checklist 写成可逐条勾选的
形态，而非散文。

## Migration Plan

分四阶段，每阶段自身可验证、可停：

1. **共享包接入验证**（最小切片）—— 建包 → 只搬 `deviceDisplayName` 一个函数 → 三端各接一次并
   跑通各自的构建/类型检查。R1/R2 在这一步暴露。此阶段结束前不搬第二个函数。
2. **共享包全量收口** —— 搬齐其余纯函数，合并三端测试，改三端 import。桌面与移动到此为止，
   后续阶段不再动它们。
3. **Web 底座** —— shadcn 接入 + token 映射 + `cn` 修复 + Lingui 阶梯 spike。此阶段不重写业务组件，
   只把地基铺好，且文档区截图对比通过。
4. **Web 表现层重写** —— 按 设备页 → 发送 → 收件箱 → 传输 → 设置 的顺序逐页重写，每页完成后
   过三道机器门禁。契约文档（D9）与设备页同批落地，使后续四页有对照依据。

**回滚**：阶段 1–2 的回滚是恢复各端本地实现（git revert 即可，无数据面影响）。阶段 3–4 只影响
Web 应用区，无持久化 schema 变更，回滚即回退提交。
