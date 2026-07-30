# Web 应用区前端（docs/app/app）

## 概览

Web 端（wasm）的**表现层**约束。内核侧看 [`libp2p-wasm.md`](libp2p-wasm.md) 与
[`net-kernel.md`](net-kernel.md)；这里只记「写 `docs/app/app` 下的 React 代码时会踩到，
而看代码本身看不出来」的部分。

宿主是 fumadocs 文档站（Next 16 App Router），构建是 **`output: "export"` 静态导出 +
`trailingSlash: true` + GitHub Pages 子路径 `basePath`**（CI 经 `PAGES_BASE_PATH` 注入
`/SwarmDrop`，本地留空）。这三条决定了下面大部分约束。

> **2026-07-30 更正**：本文件一度写成「已迁到自定义域名 `swarmapp.cn` 域名根、basePath 已
> 移除」。那是 `openspec: invite-url-canonical` 的**目标态，不是现状**——同一次提交的
> `docs/next.config.mjs` 注释里明写「swarmapp.cn 已实名但尚未备案，境内注册商可以随时停掉
> 未备案 `.cn` 的解析，所以自定义域名这一步整体延后」。代码事实：`next.config.mjs:20` 的
> `basePath` 仍在、`docs/lib/site.ts` 仍导出 `BASE_PATH`、`crates/invite` 的
> `INVITE_URL_PREFIX` 仍是 `https://swarm-apps.github.io/SwarmDrop/p/#`。
> **迁移真正落地时**要同步的是那段「换域名要同步三处」的清单，不是提前把现状改写成目标态。

导航形态（持久侧栏 + 五路由）与桌面端 breadcrumb-only 是**有意分叉**，决策记在
`DESIGN.md` 的「Navigation — Web app area」，路由清单在 `CLAUDE.md` 的「Web 端（wasm）」段。

## 运行时单例只能挂在 layout

`WebNodeBootstrap` 一个组件里做了四件事：spawn 节点、`startEventConsumption(node)`、
`startStatePoll(node)`、`ensureConfiguredRelays(node)`。它必须挂在 `app/app/layout.tsx`。

**为什么不能下放到 page**：Next App Router 下 layout 跨路由不重挂、page 每次切换都重挂。
放进 page 就是每个路由各起一份事件消费——同一条 transfer 事件被 reduce 多次，
传输进度会跳、事件日志出现重复条目。

**为什么 cleanup 里不 `closeNode()`**：`reactStrictMode: true` 开发期会 mount→cleanup→mount，
关掉会把刚 spawn（或正在 spawn）的页面级单例关掉，第二次 mount 拿到一个已关闭的实例。
节点是页面级单例（`spawnNode` 记忆化），SPA 内不显式关，标签页关闭由 wasm 的
`FinalizationRegistry` 回收。

**怎么验证「切页没重启节点」**（手测或 e2e 都适用）：注入一个采样器盯节点状态徽章文本，
连续切几轮路由后看采样集合——只要出现过「启动中」，就说明 bootstrap 被重挂了。
单看 `node_id()` 不变**不足以证明**：`spawnNode` 是记忆化的，重挂也会返回同一个实例，
node_id 照样不变，但事件消费已经翻倍了。

```js
window.__samples = new Set();
setInterval(() => {
  const el = [...document.querySelectorAll("span")].find((n) =>
    /^(未启动|启动中|运行中|关停中|启动失败)$/.test(n.innerText?.trim() ?? ""));
  if (el) window.__samples.add(el.innerText.trim());
}, 80);
// 切几轮路由后：[...window.__samples] 应当只有 ["运行中"]
```

**相关文件**：`docs/app/app/_components/web-node-bootstrap.tsx`、`docs/app/app/layout.tsx`

## 静态导出的三条硬限制

### 1. 没有服务端，重定向只能在客户端做

`next/navigation` 的 `redirect()` 与 `next.config` 的 `redirects()` 在 `output: "export"`
下都不可用。`/app` → `/app/devices` 走 `useRouter().replace()`。

**渲染真链接而不是空白 loading**：JS 未加载或被禁用时用户仍然点得进去，不会卡在一个
永远转圈的页面上。

**相关文件**：`docs/app/app/page.tsx`

### 2. 运行时 ID 不能进路由段，用 query param

`/app/transfer/[id]` 在静态导出下需要 `generateStaticParams` 预生成，而 sessionId 是运行时
UUID——永远生成不出来。传输详情因此走 `?session=…` + 就地展开，而不是照搬桌面端的
`_app/transfer/$sessionId`。

顺带一个非显见项：**选中项要参与历史列表的裁剪**。活动列表只留最近 8 条已结束会话，
若 `?session=` 指向第 20 条，用户点「查看传输」进来会看到一个「什么都没选中」的列表。
`groupSessions(projections, selectedId)` 因此显式保留选中项。

**相关文件**：`docs/app/app/_components/transfer-activity-panel.tsx`

### 3. `useSearchParams()` 必须套 `<Suspense>`

预渲染阶段读不到 query，Next 会把整页标记为需要客户端渲染；没有 Suspense 边界时
`next build` 直接失败（CSR bailout）。

**边界放在面板文件自己里，不要交给调用方**：「我读 query，所以我需要 Suspense」是组件
自身的属性，散给每个 page 手抄就等着有人忘（下一个读 query 的面板一加就是 build 红）。
写法是导出的组件只负责包边界，真正读 query 的逻辑下沉到同文件的 `XxxInner`：

```tsx
export function SendPanel() {
  return (
    <Suspense fallback={<PanelFallback>正在准备发送…</PanelFallback>}>
      <SendPanelInner />
    </Suspense>
  );
}
function SendPanelInner() {
  const peerId = useSearchParams().get(PARAM.peerId) ?? "";
  // …
}
```

**注意别把 `useSearchParams()` 写在套 Suspense 的那一层**——那样边界在读取点的外面才有用，
写在同一个组件里等于没有边界。

同一路由内 query 变化（如从设备页重复点不同设备的「发送」）**不会重挂组件**，
所以 `useState(初始值来自 query)` 之外还要一个 effect 跟随 query 同步。

**相关文件**：`docs/app/app/_components/send-panel.tsx`、
`docs/app/app/_components/transfer-activity-panel.tsx`、`docs/app/app/_components/panel-fallback.tsx`

## 路由字符串一律走 `_lib/nav.ts`，不在组件里手拼

`nav.ts` 是导航的单一事实源：`NAV.devices` / `NAV.send` … 按 key 索引（写错是编译错误，
不是运行时才炸），带参链接用 `sendToPeerHref(peerId)` / `transferSessionHref(sessionId)`，
query param 名只在 `PARAM` 里定义一次（生产方与 `useSearchParams().get()` 消费方共用）。

**组件里写 `` `/app/transfer?session=${id}` `` 会让这份事实源退化成一句注释**——改路由段时
这里改完、字面量静默失效，而静态导出没有死链检查，构建照过、线上 404。

## 底部导航的高度补偿归导航自己

`AppBottomNav` 是 `fixed` 的，它同时渲染一块等高 spacer（高度常量在同一文件）。
不要在 layout 里写 `pb-24` 这类魔数去补偿——知道高度的人和补偿高度的人应当是同一个，
否则导航行高或 safe-area 一变，那个数就悄悄失准。

## 内部导航一律 next/link（子路径下这条是致命的）

`<Link>` / `useRouter()` 会按 `trailingSlash` 补尾斜杠、自动加 `basePath` 并做预取；手写
`<a href="/app/devices">` 三样全绕过，**GitHub Pages 子路径下会整片 404**。

两条配套约束（`basePath` 仍在，见本文开头的更正，所以它们都还有效）：

- **非框架管辖的纯字符串路径要手拼 `BASE_PATH`**。`next/link` 与 `next/image` 会自动加，
  但 `<img src="/x">`、metadata 里的 `/x` 不会——`/x` 解析时会把 base path 整段替换掉。
  已知需要拼的：`docs/lib/shared.ts` 的 `appIconPath`（`${BASE_PATH}/app-icon.png`）。
- **本地验证子路径部署**：`PAGES_BASE_PATH=/SwarmDrop pnpm build`，然后 grep 导出产物里的
  `href` 确认全部带前缀：

  ```bash
  grep -o 'href="/app[^"]*"' out/app/*/index.html   # 应当为空（都该带 /SwarmDrop 前缀）
  ```

  路径大小写必须精确匹配仓库名 `SwarmDrop`（Pages 区分大小写）。

**换域名要同步三处**（跨语言没法共享常量）：`.github/workflows/docs.yml` 的
`PAGES_SITE_ORIGIN`、仓库 Settings → Pages 的 Custom domain 字段（**不是** CNAME 文件 ——
workflow 型部署会忽略它）、`crates/invite` 的 `INVITE_URL_PREFIX`，外加两份前端副本：
`mobile/src/core/invite-link.ts` 与 `docs/app/app/_lib/invite.ts`。
权威清单在 `INVITE_URL_PREFIX` 的文档注释里（含各处的性质：功能性 / 纯文案）。

**相关文件**：`docs/next.config.mjs`、`docs/lib/site.ts`、`docs/lib/shared.ts`

## 要求极小的页面不要走 Next：client component 的 baseline 就 ~150KB

配对落地页（`/p/`，openspec: invite-url-canonical）是分享链接的第一跳，而站点经 GitHub
Pages 在国内可达性不确定（域名未备案，境内 CDN 也就接不了），所以它必须在慢网络下秒开。

**Next 页面达不到这个体积**：client component 必然带上 React + framework runtime，
baseline 约 150KB gzip，而这个页面的全部逻辑是「读 hash → 给两个出口」。

做法是写成 `docs/public/p/index.html`（内联 CSS + 内联 JS）。`public/` 下的文件被静态导出
原样复制到 `out/p/index.html`，路径正好对上 canonical 邀请链接的 `/p/` 段。实测
**3.2KB gzip**，零框架、零额外请求。

代价：拿不到站点的 Tailwind 与 fumadocs 主题，明暗配色要用 CSS 变量 +
`prefers-color-scheme` 自己复刻一份。

**判据**：页面只做分流、不共享站点交互 → 走 `public/`；一旦要用站点组件或 store → 回到
Next 页面。

### 跨页面递交 capability 用 sessionStorage，不要塞进下一个 URL

落地页把邀请交给 `/app/devices` 时，存的是 `sessionStorage`（key
`swarmdrop:pending-invite`，值是完整 canonical 链接），app 区读完立即 `removeItem`。

两个理由：capability 完全不进第二个地址栏（刷新、分享、截图都带不走）；存完整链接则 app 区
不必再硬编码一份链接前缀，原样交给后端的唯一解析入口即可。落地页与 app 区同域，storage 互通。

隐私模式下 storage 可能不可用，落地页会**退回**把 payload 挂在 fragment 上传过去，所以
消费端两条路径都要读（见 `pairing-panel.tsx` 的 handoff effect）。

## `_lib/create-store.ts` 是自研 store，且不在 lint 兜底范围内

零依赖的 `useSyncExternalStore` 外部 store（当时不引 zustand 的理由写在文件头）。
它有和 zustand **完全相同的陷阱**：selector 里 `filter`/`map`/`slice` 或对象字面量派生新引用
→ 每次快照不等 → 无限重渲染（`getSnapshot should be cached`）。

**而 `pnpm check:zustand-access` 只扫仓库根的 `src/`，`docs/` 不在覆盖范围内**——
这块目前没有机器兜底，只能靠约定：selector 一律只返回原始值（数字、字符串、布尔）
或 store 内的稳定引用（整个 `projections` 对象、`pairedDevices` 数组）。

派生放组件体内（`useMemo`）而不是 selector 里。计数这类可以在 selector 里算，
因为返回的是数字——`Object.is(3, 3)` 为真，不会触发重渲染：

```ts
// ✅ 返回数字
const offerCount = useWebNode((s) => Object.keys(s.offers).length);
// ❌ 返回新数组，无限重渲染
const offers = useWebNode((s) => Object.values(s.offers));
```

**相关文件**：`docs/app/app/_lib/create-store.ts`、`docs/app/app/_lib/store.ts`、
`docs/app/app/_components/app-nav.tsx`

## 拆多路由会藏起有时效的东西，导航徽标是补偿不是装饰

单页时入站 offer 与其它内容同屏可见；拆成五条路由后它躲进了 `/app/inbox`，用户停在发送页
对「有人要发文件给我」零感知。导航项上的计数徽标（待处理 offer / 进行中传输）是这次退化的
**补偿**，不是顺手加的装饰——删掉它等于把可见性还回去。

同理，节点状态在三档断点里都必须在场：宽屏是完整 pill，图标侧栏降级成裸状态点
（文字进 `title`/`aria-label`），窄屏回到顶栏 pill。

**相关文件**：`docs/app/app/_components/app-nav.tsx`、`docs/app/app/_components/node-status-pill.tsx`

## 手测坑：Next Dev Tools 浮标会挡住底部导航

`pnpm dev` 下左下角的 Next.js Dev Tools 徽标是 fixed 定位，正好压在窄屏底部导航上，
**点击会静默落到徽标上**——表现为「点了没反应」，很容易误判成导航坏了。

窄屏底部导航的点击验证要在**生产产物**上做：

```bash
pnpm build && python3 -m http.server 3210 -d out   # 保持 trailingSlash 目录路由
```

（`npx serve out` 也可以，实测对 `/app/devices/` 与 `/app/devices` 都返回 200。）

## 面板的跨区文案会随拆页失效

单页时代的面板注释与文案里有大量「上方『连接』区」「下方『生成邀请』」这类**位置指代**。
拆页后它们全部失真。迁移面板时顺手把位置指代换成带 `<Link>` 的页面指代
（「设置页的『连接』区」），否则用户会在当前页找一个根本不在这里的东西。

## 二维码只能从 wasm 侧取，不许引 JS 二维码库

编码规范（原样编码 + 最优分段 + ECL::M + quiet zone 4 模块）单点固化在
`crates/invite/src/qr.rs`，三端共用：桌面走 Tauri 命令 `invite_qr_svg`，移动走 uniffi
`invite_qr_matrix`，Web 走 `WebNode::invite_qr_svg`。**另引 JS 库 = 第四套编码策略**，
而漂了的症状是「这端生成的码那端扫不出来」，极难归因。

几条只有踩过才知道的：

- **`{__html: svg}` 要连对象一起 `useMemo`**。React 对 `dangerouslySetInnerHTML` 只做
  引用比较，JSX 里现造的对象字面量每次都 `!==` 上一个 → 每次重渲染重设一遍 innerHTML。
  这个码面的 SVG 有三万多字节、path 里一千六百多条子命令，重解析不便宜；而对方扫完码
  拨过来那几轮 store 更新，正好都落在用户盯着码面的时候。组件再包一层 `memo`。
- **空串不能进 `{__html}`**：`{__html: ""}` 恒为真值，会渲染出一块既没码也没文字的空白卡。
  取不到 SVG 就返回 `null` 走兜底分支。
- **白卡内禁用 `text-fd-*` 主题 token**。码面固定深模块 + 白底、不随暗色主题反色
  （摄像头对反色 QR 识别差），所以卡内文字在暗色主题下会变浅灰压在白底上。用固定的
  `text-slate-*`。
- **容量不是约束，密度才是**。QR 在 ECL::M 下约 2079 字节 wire 才到顶，而浏览器邀请最坏
  327 字节（地址只来自 relay reservation，`append_invite_transports` 每桶最多留 2 条）。
  先出事的是「196px 码面下 px/模块 < 2 就扫不动」——真放宽地址上限时，掉的是扫码成功率
  而不是编码。

**相关文件**：`docs/app/app/_components/invite-share.tsx`、`crates/web/src/node.rs`、
`crates/invite/src/qr.rs`

## 剪贴板：同步失败也要接住，且状态要随内容换代重置

`docs/` 是纯浏览器环境，没有 Tauri 的 clipboard 封装（`pnpm check:clipboard` 只扫仓库根
`src/`，不覆盖这里），直接用 `navigator.clipboard`。两个坑：

- **非安全上下文下 `navigator.clipboard` 是 `undefined`，不是「有对象但 reject」**。
  取 `.writeText` 就是一次**同步** TypeError，挂在 promise 上的 `onRejected` 根本轮不到，
  按钮会一动不动地骗人。用 `try { await … } catch` 而不是 `.then(ok, fail)`。
- **「已复制」这类瞬时态要随被复制的内容换代重置**，最省事的是给按钮加 `key={内容}`。
  否则重新生成邀请后，按钮还挂着上一条的「已复制」，而剪贴板里躺的是刚被撤销的串——
  用户照着粘出去，对方拿到一条必然失败的邀请。

**相关文件**：`docs/app/app/_components/invite-share.tsx`
