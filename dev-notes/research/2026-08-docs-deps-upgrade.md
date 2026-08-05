# `docs/` 依赖升级评估（2026-08-05）

> **状态：🟡 部分落地。**
> **Next 16.2.6 → 16.3.0 已升级并全门禁通过**（它同时是 dev 内存打爆机器那个问题的正解，
> 实测数据在 [`knowledge/toolchain.md`](../knowledge/toolchain.md) 的
> 「那条放宽的代价」一节）。其余四项仍是评估结论，未动。

## 已落地：Next 16.3.0

升级动机不是「有新版本」，而是 **16.3 的 Turbopack 内存驱逐修掉了本仓 `pnpm dev` 吃光
16G 内存导致系统重启的问题**（峰值 11G+ 不收敛 → 2.4G 完成）。根因是
`turbopack.root` 指向仓库根，而那条放宽是 `packages/shared-view` 的 `link:` 依赖强制的、
删不掉。

验证过的面：`typecheck` / `test`（8 passed）/ `build`（26 静态页）/
`check:zustand-access` / `check:shared-view` 全绿，dev 实测 2.4G。

**顺带确认了两件本来担心的事**：

- `@lingui/swc-plugin` 6.6.0 与 16.3 的 `swc_core` **ABI 兼容**（`next.config.mjs` 那条
  「升 Next 要一起验」的警告已验，不匹配会是构建期 panic，实际没有）。
- `output: "export"` + `basePath` + `transpilePackages` 三条约束在 16.3 下行为不变。

**新副作用**：`next dev` 会自动生成 `docs/AGENTS.md` + `docs/CLAUDE.md`（agent 规则文件，
由 `next/dist/server/lib/generate-agent-files.js` 写入，删了会被重新创建）。**待决策**：
提交进仓 vs 加 `.gitignore`。注意本仓根目录已有一份 `CLAUDE.md`/`AGENTS.md` 体系，
`docs/CLAUDE.md` 的内容只是一行 `@AGENTS.md`。

## 未落地的四项

### 1. fumadocs 16.9.3 → 16.14.0 —— 要改代码，但**净收益**

16.14 把搜索引擎从 **Orama 换成 ZBSearch**。本仓正好踩在断裂面上：中文搜索是靠自定义
`@orama/tokenizers/mandarin` 分词器实现的，而「custom Orama tokenizers 必须迁移到 ZBSearch
等价物」。

好消息是 ZBSearch **开箱支持全语言**（Unicode 分词），所以迁移方向是**做减法**：

| 文件 | 改动 |
|---|---|
| `docs/app/api/search/route.ts` | 删掉 `tokenizer: createTokenizer()` 选项 |
| `docs/components/search.tsx` | `initOrama` → `initDB`；删两条 `@orama/*` import |
| `docs/package.json` | 可移除 `@orama/orama` + `@orama/tokenizers` 两个直接依赖 |

其他 API 路径不变；`oramaStaticClient` → `staticClient` 保留了废弃别名。
**注意**：静态搜索数据格式变了（单一 DB 而非按 locale 分片），且服务端与客户端必须同版本。

三个 fumadocs 包是 pin 死的（无 `^`），必须**三个一起升**（`fumadocs-ui` 的 peer 写死
`fumadocs-core: 16.14.0`）。

### 2. lucide-react 0.563.0 → 1.28.0 —— major，5 个图标要改

当前 `docs/node_modules` 里**已经躺着两份 lucide**（自己的 0.563 + fumadocs 拉进来的 1.21），
升级能合并掉。v1 移除 UMD 后包体降 32%。

实测（对 1.21.0 的 `.d.ts` 逐个核）本仓用到的 48 个图标里 **5 个需要改**：

| 现用 | v1 替代 | 使用点 |
|---|---|---|
| `CheckCircle2` | `CircleCheckBig` | `app/app/_components/invite-share.tsx` |
| `Loader2` | `LoaderCircle` | node-not-ready-state / download-panel / mobile-download-card |
| `MoreHorizontal` | `Ellipsis` | `app/app/_components/device-card.tsx` |
| `Fingerprint` | `FingerprintPattern` | node-panel / `(home)/page.tsx` |
| `Github` | **无替代** | about-panel、`(home)/page.tsx`×3、mobile-download-card |

**`Github` 是硬缺口**：v1 出于商标考虑整类删除了品牌图标（Chromium/Codepen/Dribbble/
Facebook/Figma/Framer/Github/Gitlab/Instagram/LinkedIn/Pocket/RailSymbol/Slack）。
本仓 5 处用它指向仓库链接，得自绘一个 SVG 组件或引 Simple Icons。

另一条行为变化：v1 起图标默认带 `aria-hidden`。本仓有些用法自己写了 `aria-hidden`，
不冲突，但**装饰性图标旁若靠 aria-label 提供语义要复核**。

⚠️ 这项与 fumadocs 升级**有耦合**：`fumadocs-ui@16.14` 依赖 `lucide-react: ^1.27.0`。
两件事一起做，才能真正只剩一份 lucide。

### 3. TypeScript 5.9.3 → 7.0.2 —— **现在不要升**

TS 7（Go 原生重写，构建快 ~10×）已于 2026-07-08 GA，但官方明说
**MDX / Vue / Svelte / Astro 的模板类型检查要等 7.1**（稳定的 programmatic API，预计
~2026-10）。`docs/` 是 fumadocs-mdx 项目，正好在这个缺口里。

中间选项是 **TS 6.0.3**——`mobile/` 已经在用（`~6.0.3`），升上去还能顺带对齐版本线
（根 workspace 仍是 `~5.8.3`）。但这不解决任何现存问题，属于纯跟进，可以等。

### 4. 杂项 patch/minor —— 随手可升，零风险

`@tailwindcss/postcss` + `tailwindcss` 4.3.1→4.3.3、`postcss` 8.5.15→8.5.25、
`radix-ui` 1.6.0→1.6.7、`react`/`react-dom` 19.2.7→19.2.8、`@types/react` 19.2.17→19.2.18、
`mermaid` 11.15→11.16。

`@types/node` 25.9.4 → 26.1.2 是 major，但 `@types/node` 的 major 跟随的是 Node 主版本，
按本机/CI 实际 Node 版本决定，不必跟最新。

`@swarm-hive/sdk` 0.2.0 → 0.4.0 是**自家包**（用在 `(home)/page.tsx`、download-panel、
mobile-download-card 三处的下载目录），升级前要先看 SwarmHive 仓的 changelog——
本文没有覆盖它。

## 建议顺序

1. ~~Next 16.3.0~~ ✅ 已做（内存问题的正解，优先级最高）
2. 杂项 patch/minor —— 一批装完跑一次门禁
3. fumadocs + lucide-react **合并成一个 PR**（两者在 lucide 版本上耦合），改动约 8 个文件
4. TS 等 7.1（~2026-10）再评估；期间若要动，升到 6.0.3 与 `mobile/` 对齐
