## Why

Web 端（`docs/app/app`）的**后端已与桌面端平权，前端还停在 demo**。`crates/web/src/node.rs:591` 的
`paired_devices()` 走的是和桌面 `list_devices` 完全同一条 `DeviceManager::get_devices(DeviceFilter::Paired)`，
返回的 `Device` 与桌面 `src/lib/bindings.ts:313` **字段字节级同构**（同源于 `crates/host/src/device.rs:436`）——
但 Web 前端只渲染了 8 个字段里的 3 个：

| `Device` 字段 | 桌面 | 移动 | Web |
|---|:---:|:---:|:---:|
| `peerId` / `status` | ✅ | ✅ | ✅ |
| `os` → 设备图标 | ✅ | ✅ | ❌ |
| `connection` + `latency` → 局域网/打洞/中继徽标 | ✅ | ✅ | ❌ |
| `trustLevel` + `trustConfirmed` → 信任徽标 | ✅ | ✅ | ❌ |
| `receivePolicy` → 策略编辑 | ✅ | ✅ | ❌ |

「这台设备现在怎么连上的、我给了它多少信任」是 PRODUCT.md 排第 2 的原则（状态诚实可见）。
数据就在手上却不呈现，是把已付出的内核成本浪费在最后一公里。

更根本的是**心智模型分叉**：桌面与移动是「设备优先」（点设备卡片 → 发送），Web 是「表单优先」
（发送页 `<select>` 下拉选设备）。三端说的不是同一个产品。

同时，三端已经在**事实上重复**纯逻辑：`device-name.ts` 三份（桌面 93 行 / 移动 61 行 / Web 30 行）、
`device-organization.ts` 两份（77 / 150 行）、`format.ts` 两份（99 / 73 行）、`update-texts.ts` 与
`update-dialog-visibility.ts` 各两份。副本越多越会漂移，而这些恰恰是**零平台依赖**的纯函数。

## What Changes

分三层推进，**表现层刻意不共享**——移动端（React Native）物理上共享不了 DOM 组件，却已证明「共享心智
模型 + 各写表现」可行；且桌面组件写死了 `min-h-[132px]`、hover 态、`glass-*` 玻璃拟态与 `···` 悬浮菜单，
搬进 375px 手机浏览器是负资产。

### L1 — 抽出跨端共享的纯视图逻辑包

- **新增** `packages/shared-view/`（根 pnpm workspace 的 member），收口三端重复的零平台依赖纯函数：
  设备显示名归一、别名/分组解析与排序、信任级别归一与可发送判定、字节/延迟/时长格式化。
- **BREAKING**（仅内部）：`src/lib/device-name.ts`、`src/lib/device-organization.ts`、`src/lib/format.ts`、
  `mobile/src/lib/device-name.ts`、`mobile/src/lib/device-organization.ts`、`mobile/src/core/device-trust.ts`、
  `docs/app/app/_lib/device-name.ts`、`docs/app/app/_lib/format.ts` **删除本地实现，改为 re-export 共享包**。
- **修改** 根 `pnpm-workspace.yaml`（`packages: [., packages/*]`）与 `docs/pnpm-workspace.yaml`
  （docs 经 `link:` 引用仓库根的 `packages/shared-view`，与 `swarmdrop-web` 的现有做法同理）。
- 共享包**不含任何 React / RN / DOM 依赖**，只依赖 `Device` 型别的结构（结构化类型入参，不 import 三端 bindings）。

### L2 — 把三端交互契约写进 `DESIGN.md`，成为 review 判据

- **新增** `DESIGN.md` 的「设备卡片规格」：必须呈现的信息位（设备图标 · 显示名 · 在线点+文案 ·
  次级身份行 · 信任徽标 · 连接徽标+延迟 · 发送动作 · 溢出动作），以及各端允许的形态差异。
- **新增** 「发送入口规格」：发送 SHALL 从设备进入；发送页的目标选择器只作为直达链接的落点，
  不作为主路径。三端一致。
- **新增** 「跨端一致性判据」：新增设备相关 UI 时的 review checklist。

### L3 — Web 端表现层重写

- **修改** `docs/` 接入 shadcn/ui：新增 `components.json`（`new-york` / `neutral` / `rsc: true`），
  装入 `button` / `dialog` / `alert-dialog` / `dropdown-menu` / `badge` / `card` / `select` / `sheet` /
  `separator` / `skeleton` / `tooltip`。
- **BREAKING**（仅内部）：`docs/lib/cn.ts` 从 `export { twMerge as cn }` 改为 `clsx + twMerge` 组合
  （shadcn 组件依赖条件类名合并，当前实现不支持）。
- **新增** `docs/app/global.css` 的 `@theme` 映射层：把 fumadocs 的 `--color-fd-*` 映射成 shadcn 语义
  token（`--color-background` / `--color-border` / `--color-card` / `--color-muted-foreground` …），
  并把 `--brand` 对齐桌面 `src/index.css` 的 oklch 值。文档区不受影响。
- **重写** Web 设备页：`device-list.tsx` 的 `<ul><li>` 文本行 → 响应式设备卡片网格，补齐 L2 规格要求
  的全部信息位；新增信任策略编辑与别名/分组（`receivePolicy` 数据已在）。
- **重写** Web 发送入口：从「表单优先」改为「设备优先」——设备卡片是主路径，`/app/send?peerId=`
  成为落点而非入口。
- **修改** Web 收件箱与传输页：改用主从布局，断点复用桌面已定死的 `(min-width: 920px)`。
- **修改** Web 布局基准改为**移动优先**：单栏为基线，≥920px 渐进为双栏。

### L4 — Web 端国际化

- **新增** Lingui 接入 `docs/`（SWC plugin + `zh` / `zh-TW` / `en` 三 locale，与桌面同 catalog 体例）。
- **BREAKING**（仅内部）：`docs/app/app/**` 下全部硬编码中文串改为 `<Trans>` / `t\`\``。
  文档站正文（`docs/content/`）与营销页不在本次范围内。

### 非目标

- 桌面端与移动端的**表现层不改**。契约是对它们现状的追认，不是要求它们重画。
- 不做兼容层、不做渐进迁移：Web 端无真实用户，`docs/lib/cn.ts` 与 `_lib/*.ts` 直接换掉，不双写。
- 不引入第二套 token 体系；不给 docs 文档区换皮。

## Capabilities

### New Capabilities

- `shared-view-logic`: 跨端共享的纯视图逻辑包——归属边界（什么该进、什么不该进）、三端引用方式、
  「不得再长本地副本」的约束，以及共享包对平台依赖的零容忍。
- `device-presentation-contract`: 设备在 UI 中的呈现与操作契约，对桌面 / 移动 / Web **三端同时生效**——
  必须呈现的信息位、发送入口必须从设备进入、允许的形态差异边界。
- `web-app-shell`: Web 应用区外壳——移动优先布局基准、920px 主从断点、shadcn/ui 组件底座与
  fumadocs token 映射层、运行时单例与静态导出的既有约束在新形态下依然成立。
- `web-i18n`: Web 应用区国际化——locale 集合、catalog 位置、与桌面 catalog 的分工、静态导出下的
  locale 选择与持久化。

### Modified Capabilities

（无。桌面端 `home-device-hub` / `pairing-page` / `transfer-detail-page` 的既有需求不变——
`device-presentation-contract` 是对它们现状的成文化，不修改其行为。）

## Impact

**新增目录**
- `packages/shared-view/`（新 workspace member）
- `docs/components/ui/`（shadcn 组件）
- `docs/src/locales/{zh,zh-TW,en}/messages.po`（Lingui catalog）

**受影响代码**
- 根 `pnpm-workspace.yaml`、`docs/pnpm-workspace.yaml`、`docs/package.json`、`docs/next.config.mjs`
  （Lingui SWC plugin + `transpilePackages`）
- `src/lib/{device-name,device-organization,format}.ts` + 其 `.test.ts`
- `mobile/src/lib/{device-name,device-organization}.ts`、`mobile/src/core/device-trust.ts`
- `docs/app/app/**` 全量（19 个组件 + 5 个 page + `_lib/`）
- `docs/lib/cn.ts`、`docs/app/global.css`
- `DESIGN.md`（新增三节）

**依赖变更**
- `docs/`: 新增 `@lingui/core` / `@lingui/react` / `@lingui/macro` / `@lingui/swc-plugin`、
  `clsx`（已在）、`tw-animate-css`；`@lingui/cli` 入 devDependencies
- 根: 新增 workspace member，无第三方依赖新增

**门禁**
- `pnpm check:zustand-access` 的规则 B 已覆盖 `docs/app/app`，重写后必须继续通过
- `docs` 的 `pnpm typecheck` 与 `next build`（静态导出）必须通过——`useSearchParams()` 的
  Suspense 约束在新布局下依然成立
- 新增：共享包的单测（从三端现有 `.test.ts` 合并去重）

**Rust 侧**
- `crates/web`：新增 `update_paired_device_policy` 导出（薄壳，业务逻辑在 core 的
  `paired_devices::set_receive_policy`，与桌面命令同一条路径）+ `serialize::from_js`
  与两条 wasm 回归守卫。原提案把「不改任何 Rust crate」列为非目标，实施中经确认放开——
  信任策略编辑在 wasm 侧本来就没有写入路径，不补它这条需求做不了。
- **其余 Rust crate 零改动**：数据面本来就已就绪。

**桌面 / 移动的渲染输出：三处刻意的收敛，其余不变**

收口一份共享实现时，两端原本不同的地方必须有一端让步。让步的三处都是**朝更诚实的那一侧**：

| 处 | 原桌面 | 原移动 | 收敛后 |
|---|---|---|---|
| `formatSpeed(0)` | `0 B/s` | `—` | `—`（0 B/s 与「还没开始」在进度条上分不出来） |
| `calcPercent` 溢出 | 不夹取（能显示 137%） | 夹到 100 | 夹到 100 |
| `deviceIdentityHint` hostname 为空 | `未知设备 · 短ID`（i18n-free 模块里的硬编码中文） | `短ID · 短ID` | 只给 `短ID` |

其余只换 import 路径，行为不变，靠现有 122 条测试钉住。

**不受影响**
- `docs/content/` 文档正文与营销页
