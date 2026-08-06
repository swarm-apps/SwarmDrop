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

### 第二个单例：环境层（WebGL 极光，2026-08-05 起）

`AppAmbientBackground` 同样只能挂 layout，但理由更硬：它持有 **WebGL context**，
浏览器对同时存活的 context 数有上限，超了会**静默丢掉最老的那个**——表现是切几轮路由后
背景变白，没有任何报错。

它拆成两个文件，两条都省不掉：

- `_components/ambient-canvas.tsx` —— 本体（着色器 + 两个 Renderer），**默认导出**
- `_components/app-ambient-background.tsx` —— 只做 `dynamic(() => import("./ambient-canvas"), { ssr: false })`

`ssr: false` 只能写在 client component 里（layout 是 server component，写那儿直接构建报错），
而 `dynamic()` 必须 import **另一个模块**代码分割才发生——写在同一个文件里，ogl 与两段着色器
照样进首屏 chunk。分割是有意义的：实测该 chunk **15.6 KB gzip**，且不在
`/app/devices/index.html` 引用的首屏 chunk 里——用户打开标签页要的是拉 `_bg.wasm` 起节点，
不是看背景。验证方法：

```bash
grep -o 'chunks/[a-z0-9]*\.js' out/app/devices/index.html | sort -u | while read c; do
  grep -q auroraGlow "out/_next/static/$c" && echo "⚠️ 极光进首屏了: $c"; done
```

着色器与 `*_CONFIG` **逐字取自桌面** `src/components/layout/app-ambient-background.tsx`，
不要单边改（分叉在截图里看不出来，两边都是「一片流动的光」）。允许分叉的只有加载方式、
DPR、帧率、层不透明度与遮罩，理由与数值见 DESIGN.md 的 Ambient WebGL Background 一节。

## 「Web 端没有 X」之前先分清：是浏览器没有，还是绑定没导

三端对齐时反复撞到的一个坑。`crates/web` 只导出了 `WebNode` 上手动写出来的那些方法，
**内核有的能力不等于 JS 拿得到**。2026-08-05 判错过一次：把「连接数」和「NAT 状态」
一起当成「浏览器没有的概念」，其实两者的成因完全不同。

| | 内核有没有 | wasm 下成立吗 | 结论 |
|---|---|---|---|
| `connected_peers` | 有（`Endpoint::watch_conns`，无 cfg 门控） | 成立 | **只是没导出**。2026-08-05 补了 `WebNode::connected_peers()` |
| `nat_status` | 有（`Endpoint::watch_nat`） | **不成立** | `WatchSenders::nat` 上挂着 `cfg_attr(wasm_browser, expect(dead_code))`——唯一写入点是 autonat 事件，而 autonat 是 native-only，wasm 下恒为 `Unknown` |

判定方法：在 `crates/net` 里 grep 那个 watch 的**写入点**有没有 `cfg`。有 → 真不成立，
别在界面上摆一个永远不变的占位；没有 → 加个 `#[wasm_bindgen]` 方法就有了。

「浏览器有 WebRTC，为什么没有 NAT 状态」这个问法要拆开：WebRTC 给的是 ICE 候选，
而 `natStatus` 回答的是「我公网可拨吗」——浏览器压根不监听端口，这个问题对它不成立。
它的等价物是 **circuit 预留有没有建起来**（`relays_state()` / `selectReservation`）。

**加导出的代价**：改 `crates/web` 公开面要重跑三条生成链路且都要入库，见下文
「改 `crates/web` 的公开面，有三条生成链路要重跑且都要入库」。`pnpm build:wasm` 在 macOS 上
需要 `CC` / `AR_wasm32_unknown_unknown` 指向 homebrew llvm。

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

> **2026-08-04 更新**：历史列表的 8 条硬截断**已删除**，`groupSessions` 的第二个参数
> 从 `selectedId` 换成了筛选档（全部 / 进行中 / 可恢复 / 已结束，与桌面同名同义）。
>
> 原先的做法是「只留最近 8 条已结束会话，并显式保留选中项，免得深链指向第 20 条时进来
> 看到一个什么都没选中的列表」。那个补丁修的是症状：**第 9 条起在 UI 里根本够不着**，
> 只有带 `?session=` 的深链能捞回来，而那条链接的唯一生产者是发送页刚发完那一下。
> 当时的理由「再多就该去收件箱看结果」也不成立——收件箱只有**接收**方向，发出去的
> 历史在那里一条都没有。
>
> 连带的一条仍然有效、且更普遍：**深链要么保证能到达，要么就别给**（同
> `inboxItemHref` 的 `archived` 参数）。现在它由「不截断」来保证。

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

## ~~底部导航的高度补偿归导航自己~~（2026-08-04 起不再适用）

> 原文：`AppBottomNav` 是 `fixed` 的，它同时渲染一块等高 spacer，「知道高度的人和补偿高度的
> 人应当是同一个」。
>
> **这条约定连同那个高度常量一起没了。** 应用外壳改成受限高度（`h-dvh`）之后，侧栏 / 顶栏 /
> 底栏都是 flex 里的 `shrink-0` 子元素，滚动只发生在 `main` 里——不需要 `fixed`，也就不需要
> 补偿。没有补偿，就没有失准的可能。详见下面「应用外壳必须是受限高度」一节。

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

## store 是 zustand（2026-08 起），selector 派生已有机器兜底

`_lib/store.ts` 用 `zustand/vanilla` 的 `createStore` + `zustand` 的 `useStore`。
此前是自研的 `_lib/create-store.ts`（零依赖 `useSyncExternalStore`），迁移时**调用面几乎没动**
——两者的 `getState / setState / subscribe / getInitialState` 形状本就一致。

陷阱不变：selector 里 `filter`/`map`/`slice` 或对象字面量派生新引用 → 每次快照不等 →
无限重渲染。**但现在 `pnpm check:zustand-access` 会拦**（规则 B，覆盖 `src/` 与
`docs/app/app`）。派生放组件体内的 `useMemo`；计数这类可以留在 selector 里，
因为返回的是数字——`Object.is(3, 3)` 为真：

```ts
// ✅ 返回数字
const offerCount = useWebNode((s) => Object.keys(s.offers).length);
// ❌ 返回新数组，无限重渲染
const offers = useWebNode((s) => Object.values(s.offers));
```

### 迁移时唯一改了语义的地方：`setState` 的「内容没变」写法

自研版逐键浅比较，`return {}` 天然不通知；**zustand 判的是 `Object.is(partial, state)`**，
`{}` 是个新对象、判不等 → 照常广播一轮，所有 selector 白求值一次。

所以「内容没变」一律 **`return s`**（返回 state 本身），不要 `return {}`。迁移时 6 处全部改过。
空对象在 zustand 下不报错、类型也合法，纯靠约定守——`setInboxItems` / `setOffers` /
`setPairedDevices` 那三个内容比较守卫都依赖它。

**相关文件**：`docs/app/app/_lib/store.ts`、`scripts/check-zustand-store-access.mjs`

## 从 chrome 上拿掉的东西，必须在用户已经在的地方重新出现

单页时入站 offer 与其它内容同屏可见；拆成五条路由后它躲进了 `/app/inbox`，用户停在发送页
对「有人要发文件给我」零感知。导航项上的计数徽标是这次退化的**补偿**，不是顺手加的装饰。

**2026-08-05 又发生了一次，形态不同**：常驻导航收敛成三项（设备 / 收件箱 / 设置，对齐移动端
tab）时，「传输」离开侧栏，它那枚活跃计数徽标也跟着没了。补偿是设备页的
`active-transfers-section.tsx`——**几行真内容而不是一个数字**：既然设备页本来就是首页，
直接把进行中的会话连进度一起摆出来，比在导航上挂一个「3」更有用。

所以这条规矩比「徽标不是装饰」更普遍：**动导航结构时，先问被拿掉的那一项在补偿谁的可见性，
再决定补偿放哪儿**。补偿的形态可以变，存不存在不能变。

两条配套：

- **子页面必须能高亮父项、且页面上有返回出口**。`_lib/nav.ts` 的 `parent` 字段同时喂两处：
  `activeNavHref()`（侧栏在 `/app/send`、`/app/transfer` 上高亮「设备」）与 `PageHeader` 的
  返回链接。少了前者，离首页最远的两个页面上侧栏三项全灭——常驻导航的全部意义就是随时回答
  「我在哪」；少了后者，用户只能按浏览器后退。
- 节点状态在三档断点里都必须在场：宽屏是完整 pill，图标侧栏降级成裸状态点
  （文字进 `title`/`aria-label`），窄屏回到顶栏 pill。

**相关文件**：`docs/app/app/_lib/nav.ts`、`docs/app/app/_components/app-nav.tsx`、
`docs/app/app/_components/active-transfers-section.tsx`、`docs/app/app/_components/page-header.tsx`

## 节点可以停可以起之后，三处「隐含节点只启动一次」的假设会同时失效

Web 端此前节点只在 layout 挂载时 spawn 一次、永不关停。节点状态弹窗（`node-status-dialog.tsx`）
加了启停之后，下面三件事一起变成了 bug 面——**都不是编译错误，两条还没有任何报错**：

| # | 原本的写法 | 节点能重启之后 |
|---|---|---|
| 1 | `event-dispatch.ts` 用一个模块级布尔 `consuming` 防重复取流 | `events()` 每个实例只能取一次，但守卫记的是「有没有人在消费」。旧流的 `done` 还没落地时新实例就撞上「已经在消费了」被静默跳过——**节点在跑、传输事件一条不到、进度永远 0** |
| 2 | 启动序列（spawn → 回补历史 → 三条订阅 → relay 登记 → 置状态）写在 `WebNodeBootstrap` 的 effect 里 | 弹窗那条路径只能照抄一遍，漏任何一步都不报错 |
| 3 | 各页空态判 `status !== "running"` 就说「正在启动节点…」 | 用户刚亲手停掉节点，界面却告诉他正在启动，而他等不到任何结果 |

修法分别是：守卫**按节点实例换代**（`current?.node === node` 才幂等，换实例先停旧的）；
启动序列收进 `_lib/node-lifecycle.ts`（`startNodeRuntime` / `stopNodeRuntime`，两个调用方共用）；
空态收进 `node-not-ready-state.tsx` 按四种状态分说，且 `idle` / `error` 直接给一颗启动按钮
（而不是「去点某个角落那枚徽章」——那种方位指代在窄屏就是错的，徽章那时在顶栏）。

还有一条是 review 时才抓出来的：**装配中途失败必须回滚，连节点一起**。

启动序列有五步，任何一步都可能抛。原先的写法是一次性赋值

```ts
subscriptions = { stopPoll: startStatePoll(n), stopRelayWatch: startRelayWatch(n) };
```

——后一个调用抛错会让整条赋值作废，而前一个已经起了的 `setInterval` 再也收不回来，且
`subscriptions` 仍是 `null`，下一次启动会若无其事地再起一个。改成往数组里逐个 push，
失败时照着回滚。

更隐蔽的是**节点本身也要关掉**：spawn 成功而后续装配失败时它还活着，而 `spawnNode()` 是
记忆化的——下一次启动拿到同一个实例，`events()` 却已经被上一次取走过了（每实例只能取一次），
于是那条流再也接不上：节点看着在跑，传输进度永远是 0。这条与上表第 1 行是同一个坑的两个入口。

另外三条只有写的时候会想到的：

- **启停要串行化**。两者动的是同一份订阅句柄，交叠执行时后发的 stop 会停掉先发的 start 刚挂上的
  订阅，留下一个「状态是运行中、却没有任何事件进来」的节点。UI 按钮的禁用条件将来会变，
  机制兜底不会（`node-lifecycle.ts` 的 `transition` 队列）。
- **停止的顺序是「停订阅 → 关节点 → 清运行态」**。轮询打在已关停的节点上会逐 tick 抛错；
  而 `reset()` 若排在关节点之前，界面先一步清空、关停还在途——中间那段用户看到的是一个
  「没有任何设备的运行中节点」。
- **`WebNodeBootstrap` 的 effect 不再有 cleanup**，这是刻意的：运行时是页面级单例，StrictMode 的
  mount→cleanup→mount 不该把它停掉再起一遍。启动幂等，关停只有一个来路——用户显式停。
  此前那个 `cancelled` 标志兜的就是这件事，现在由模块级幂等兜。

**验证过**（2026-08-05，静态产物 + 真 relay）：停 → 起一轮之后节点 ID 不变、relay reservation
重建、`relays_changed()` 与轮询都重新接上、JS 侧零报错。

**相关文件**：`docs/app/app/_lib/node-lifecycle.ts`、`docs/app/app/_lib/event-dispatch.ts`、
`docs/app/app/_components/node-status-dialog.tsx`、`docs/app/app/_components/node-not-ready-state.tsx`

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

## 改 `crates/web` 的公开面，有三条生成链路要重跑且都要入库

前端拿到的类型与方法全部是生成物，链路有三条、彼此不串联，**跑漏任何一条都是前端拿着
过期契约**。改了 `crates/web/src/types.rs` 或 `node.rs` 的公开面就把三条按顺序走一遍：

```
crates/web/src/types.rs
  ──(cargo test -p swarmdrop-web --features specta --test specta_export)──>
      crates/web/bindings/bindings.ts        ← node.rs 用 include_str! 整体注入 .d.ts

crates/web/src/node.rs
  ──(cd docs && pnpm build:wasm  →  wasm-pack build --target web)──>
      docs/packages/swarmdrop-web/{swarmdrop_web.js, .d.ts, _bg.wasm, README.md}

docs/app/app/_lib/view-types.ts        ← 手工再导出新类型（它刻意不手写镜像，只 re-export）
```

**顺序不能反**：第二条会把第一条的产物 `include_str!` 进 wasm-bindgen 的 .d.ts，先跑
`build:wasm` 再生成 bindings.ts 等于白跑。

三处各自漏掉的症状不一样，且**都不是编译错误**：

- **漏在 specta 注册**：`tests/specta_export.rs` 的 `Types::default().register::<..>()` 链
  **不是自动扫描**，新 DTO 要手动挂上去（并在 `lib.rs` 的 `pub use types::{..}` 里导出，
  那个测试从 crate 根导入）。漏了则 bindings.ts 里没有该类型，而
  `#[wasm_bindgen(typescript_type = "XxxJson")]` 里的类型名是个**字符串，wasm-bindgen 不校验**
  —— Rust 侧照样编过，等到前端 import 才炸。
- **漏跑 `pnpm build:wasm`**：新增方法时 `pnpm typecheck` 会说 `node.cancel_send` 不存在，
  还算好归因；**改了实现而签名没变**时 typecheck 与 `pnpm build` 全绿，跑起来是旧行为，
  只能靠「碰过 `crates/web` 就必重跑」的纪律兜。产物三份（`.js` / `.d.ts` / `_bg.wasm`）
  都在 `git ls-files` 里，`git status` 看不见 `_bg.wasm` 变化就是没跑。
- **漏加 `view-types.ts`**：报「模块没有导出的成员」。注意它可以被**绕过**——组件直接从
  `swarmdrop-web` 导也一样编过（`pairing-panel.tsx` 的 `InviteListItemJson` 就是这么写的），
  所以这个列表会慢慢变得不全，别把「编译过了」当成「已经加了」。

**相关文件**：`crates/web/tests/specta_export.rs`、`crates/web/src/node.rs`、
`docs/app/app/_lib/view-types.ts`

## 加 IndexedDB object store 要同改三处，漏第三处只在运行时报错

`crates/web/src/idb.rs` 里加一张新表（`inbox` 是第四张，`DB_VERSION` 3 → 4）必须同时动三处，
**少任何一处都编得过**：

| # | 位置 | 漏掉的症状 |
|---|---|---|
| 1 | `pub const XXX_STORE: &str = "…"` | 编译错误（唯一会红的一处，也是最没威胁的一处） |
| 2 | `DB_VERSION` 提一档 | `onupgradeneeded` **根本不触发** ——老库里那张表永远不存在 |
| 3 | `STORES` 表加一行 `(XXX_STORE, 引入时的版本号)` | 版本提了、回调也进了，但**不建这张表** |

2 与 3 都只在**运行时**暴露，且症状一模一样：第一次读写这张表拿一个 DOM 异常
（`NotFoundError: One of the specified object stores was not found`）。
`cargo check` / `check-wasm.sh` / `pnpm build` 全绿，跑起来才炸 —— 而这三样是平时唯一的门。

### 换字段含义同样要提版本号（2026-08-05）

上面说的是「加表」。**改一张已有表的记录格式**是另一件事，而且更隐蔽：
`inbox` 行的 `title` 从「拼好的整句标题」换成「首文件名」时，v4 与 v5 的行结构
**逐字段相同**——旧行会**成功**反序列化，然后被前端再拼一次后缀，显示成
「a.pdf 等 3 个文件 等 3 个文件」，**无 warn 无报错**。

所以**不能指望反序列化失败来过滤旧行**，判据只能是版本号；而 `onupgradeneeded` 是唯一
知道 `old_version` 的地方。`STORES` 表里每个 store 的第二个数字就是它的**记录格式版本**：
低于它的旧行在升级时整表丢弃。

⚠️ 这个数**不是 `DB_VERSION` 的别名**。下次为别的原因提版本号（比如加一张新表）时，
各 store 已写下的行仍然是好的；跟着 `DB_VERSION` 走会把它们一并清掉，而且清得悄无声息。

两条配套：

- **建表那半只做「建缺失的 store」，逐个判存在性、不按版本号分支。**
  老库升级与新库首建因此走同一段代码。丢弃那半才看版本号，两者在同一个循环里各司其职。
  拿不到 `IdbVersionChangeEvent` 时一个都不丢——多留脏行只是难看，误删是真丢数据。
- **schema 变更不写迁移 / 回填 / 双写。** Web 端目前没有真实用户，
  升版本号只为把新表建出来，旧数据直接丢弃（`inbox-store-port-completion` 的 design D7）。
  实际观感是「传输历史满的、收件箱空的」，那是**预期结果不是 bug** ——
  验收时容易误报，推导记在
  [`storage-abstraction.md`](storage-abstraction.md) 的「schema 变更直接换」一节。

**相关文件**：`crates/web/src/idb.rs`、`crates/web/src/store.rs`、`crates/web/src/inbox.rs`

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

## `useAsyncAction` 的 seq 不覆盖「转入无动作态」

`_lib/use-async-action.ts` 用一个 seq 计数器丢弃过期结果，但**它只在 `run()` 里递增**——
覆盖的是「新一轮顶掉旧一轮」，不是「退出这件事」。调用方若在不发起新调用的情况下离开
（检索框被清空、面板收起、条件不再满足），在途 promise 的 `mySeq === seq.current` 仍成立，
resolve 时会把结果写回一个已经不该显示它的界面，顺带把上一次的 `error` 也留在原地。

这条路径必须显式 `cancel()`（#108 为此给该 hook 补了这个方法）。

**它比手写的 `cancelled` 标志少覆盖一种情况**，这点反直觉：effect 里的 `let cancelled = false`
+ cleanup 置真，天然覆盖「依赖变了」与「卸载」；而 seq 只认「又调了一次」。所以把手写取消
换成这个 hook 时，要单独确认「不再调用」的那条分支有没有人管——#108 的检索就是这么漏的，
症状是搜索框已清空、列表却停在上一次的命中结果里。

**相关文件**：`docs/app/app/_lib/use-async-action.ts`、`docs/app/app/_components/receive-panel.tsx`

## 内容比较守卫会随域模型悄悄失真

store 里给列表快照做的「内容没变就不换引用」守卫（`inboxItemsEqual` / `devicesEqual`）都是
**手写的字段清单**。它写下来的那一刻编码的是当时 UI 会读的字段，而 UI 后来会读更多。

`inboxItemsEqual` 原本比 `id / receivedAt / missing / files.length`，注释里还写明了理由
（「归档 / 软删的条目根本不在列表里，无需比时间戳」）——那句话在收件箱只能读不能写的时候
成立。#108 给它加了三个写入口之后，两条路径立刻漏网：下载后标已读（集合不变，只有
`lastOpenedAt` 变）、以及「显示已归档」开着时取消归档（集合同样不变）。两者都被判等丢弃，
UI 停在旧值不动。

**症状不是报错，是「点了没反应」**，而且 `setState({})` 本身是合法路径，任何门禁都不会红。

**规矩**：往 DTO 上加可变字段、或给一张表加写入口时，回头看一眼它的比较守卫。
判断依据是「这个字段会不会在集合不变的前提下单独变化」——会，就必须进清单。

**相关文件**：`docs/app/app/_lib/store.ts`

## 组件底座是 shadcn/ui，token 走映射层（2026-08 起）

应用区不再手写原生元素 + fumadocs 的 `--color-fd-*`，改用 **shadcn/ui**（`docs/components.json`，
`new-york` / `neutral`）。组件是**从桌面 `src/components/ui/` 复制过来的**，不是 CLI 装的——
shadcn CLI 在 Node 24 下起不来（传递依赖 `@modelcontextprotocol/sdk` 引 `zod/v3` 子路径，
3.4.0 与 latest 同样报 `ERR_PACKAGE_PATH_NOT_EXPORTED`）。复制反而更好：桌面那份已经是当前形态
（统一 `radix-ui` 包，docs 早就装了同一个），离线确定，且两端组件行为逐字一致。
唯一要改的是 `@/lib/utils` → `@/lib/cn`。

### ⚠️ 前缀属性要写在标准属性**前面**，否则标准那条会被构建删掉（2026-08-06）

源码里规规矩矩写了两条，顺序是「标准在前、前缀在后」：

```css
.glass-panel, … , .glass-rail {
  backdrop-filter: blur(var(--glass-blur, 18px)) saturate(145%);
  -webkit-backdrop-filter: blur(var(--glass-blur, 18px)) saturate(145%);
}
```

产物里**只剩前缀那条**。成因是 Lightning CSS（Tailwind v4 内置）把前缀版与标准版当作
**同一个属性的两次声明**，去重时只留**最后一条**。把顺序调过来（`-webkit-` 在前）
两条就都在了。

> **归因更正。** 这条最初写的是「Lightning CSS 按 browserslist 裁剪前缀族，而 `docs/`
> 没配 browserslist、吃的是 Next 默认目标（含 Chrome 64 / Safari 12）」，并据此加了
> `docs/.browserslistrc`。**隔离实测推翻了它**：移走那份配置后，只要顺序是对的，两条
> 照样都在产物里；Tailwind v4 本来也不读 browserslist（它有固定的现代目标）。
> browserslist 那份配置留着是因为它自身合理（见该文件注释），不是因为它修了什么。
>
> 教训与这条缺陷本身同样值得记：**「产物少了东西」有两类成因——按目标裁剪、按重复去重**，
> 两者的现象一模一样。先做隔离实验（移走一个变量再构建）再下结论，别拿第一个说得通的
> 机制当答案。

**而现代 Chrome 已经不认 `-webkit-backdrop-filter`**（实测：手写一条前缀声明，computed
`backdropFilter` 仍是 `none`，且 computed style 里根本没有 `webkitBackdropFilter` 这个键）。

净结果：**整个 Web 应用区的玻璃在 Chrome 上完全失效了**，而且失效得极其隐蔽——同一个块里的
`background` 照常生效，只有模糊那一半没了，玻璃退化成一块半透明色板。桌面端没这个病，
它跑在 WKWebView（Safari 内核）里，前缀版本本就是那儿的正解。**两端观感差异的大头在这里。**

排查时最容易走错的一步是**怀疑 token**：两端的 `--glass-*` 值逐字相同，对着它们比对半天也
看不出问题。判据要落在 computed style 上：

```js
getComputedStyle(document.querySelector('.glass-rail')).backdropFilter
// "none" = 规则没生效（不是 token 不对，也不是 prefers-reduced-transparency）
```

**修法就是调顺序**：

```css
.glass-… {
  -webkit-backdrop-filter: blur(…) saturate(145%);  /* 前缀在前 */
  backdrop-filter: blur(…) saturate(145%);          /* 标准在后，才留得下 */
}
```

**关闭也要走同一条通道。** `@media (prefers-reduced-transparency: reduce)` 里那条
`backdrop-filter: none` 中了同一枪，于是无障碍降级在现代 Chrome 上**根本关不掉玻璃**——
用户开了「减少透明度」，模糊还在。那种失灵是不会有人来报的。

**桌面端 `src/index.css` 四处同样写反了**，只是没暴露（WKWebView 是 Safari 内核，前缀版
正是那儿的正解）。会暴露的场合有两个：在 Chrome 里跑 `pnpm dev` 调前端，以及 Safari 18 起
标准属性才是首选。已一并调顺序。

**新增任何有前缀历史的现代属性都要按这个顺序写，并去产物里 grep 一次确认**：

```bash
curl -s "$(浏览器里读 link[rel=stylesheet].href)" | grep -o '[^-]backdrop-filter'
# dev 与 build 两套管线都要看，它们的 CSS 处理不是同一条
```

### token 映射层：只新增别名，绝不改写 `--color-fd-*`

`docs/app/global.css` 有一层 `@theme inline`，把 fumadocs 的 `--color-fd-*` 映射成 shadcn 要的
无前缀语义 token。**「文档区零影响」的全部依据就是「只新增别名」**——文档区读的仍是原变量，
读不到这些别名。

三条必须知道的：

- **`primary` 走品牌色**（`--brand-solid` / `--brand-ink`），不跟 fumadocs 的 primary——
  应用区的主按钮是产品身份的一部分。
- **fumadocs 没有的自给**：`destructive` / `input` / `radius`。它们不参与文档区呈现。
- **`@layer base` 的默认边框色必须限定作用域**：shadcn 组件写裸 `border`，需要
  `* { @apply border-border }` 兜底。桌面那份是全局 `*`（整个 app 都归它管），**这里不能照抄**
  ——全局套用会把文档区每个元素的默认边框色一起换掉。作用域锚点是 layout 根节点上的
  `data-swarmdrop-app` 属性。

### ⚠️ `size="icon"` 的 44px 触达档，在只于 `md:` 以上渲染的组件里**永远不命中**（2026-08-06）

`button.tsx` 的每个 size 都是「移动端 44px、桌面收窄」的形态，`icon` 是 `size-11 sm:size-9`。
那条规则本身是对的（它属于按钮而不属于调用点，文件头注释还写着「从桌面同步这个文件时不要
覆盖掉这段」），**但断点是 `sm:`（640px）**。

于是任何只在 `md:`（768px）以上渲染的组件——侧栏 `RailTools`、`AppSidebar` 里的一切——
它可见的每一个视口都已经越过 `sm:`，拿到的恒是 `size-9`（36px）。写 `size="icon"` 会让人
以为触达达标了，实际没有，而且**没有任何门禁会提**。

这不是「改一下 class 就行」：图标侧栏那一档宽 64px（`md:w-16`），减掉容器的 `p-3` 只剩 40px，
44px 的按钮塞不进去。要满足触达标准得先加宽侧栏，而 64px 是 DESIGN.md 定死的三档形态之一。

**规矩**：在侧栏（或任何 `md:` 才出现的容器）里放图标按钮时，
① 仍然用 `size="icon"`（尺寸规则该归按钮）；
② 但**别在注释里声称它拿到了 44px**——如实记下这一档是 36px 与原因，否则下一次 a11y 审计
会因为看到 `size="icon"` 而跳过这里。
③ 真要修，入口是侧栏宽度，不是按钮常量。

### 品牌色与桌面同源，写成同一组 oklch 表达式

`--brand` / `--brand-solid` / `--brand-ink` 分别对应桌面 `src/index.css` 的
`--brand` / `--primary` / `--primary-foreground`。此前这边是 hex、那边是 oklch，实测**本来就是
同一组颜色**（最大通道差 0–1，取整误差）——但「同一组」这件事只能靠人去转换才看得出来。
现在两份文件的这几行可以直接肉眼比对，改一边漏另一边会显眼。

**相关文件**：`docs/components.json`、`docs/app/global.css`、`docs/lib/cn.ts`

## `scrollbar-gutter: stable` 会在应用区右边留一条永远空的死边

`global.css` 里 `html { scrollbar-gutter: stable }` 是给**文档站**的：长短不一的文档页之间
跳转时不左右抖。但应用外壳是 `h-dvh overflow-hidden`，**文档永远不溢出**，那条槽从头到尾
是空的，Chrome 还在里面画一条滚不动的轨道——看起来就是「滚动条没贴边、悬在那儿」。

判据（在应用区任一路由 eval）：

```js
// 死边存在时：deClientW 1440 / bodyW 1425（差的 15px 就是那条槽）
// 且 docScrollH === docClientH ——> 文档根本没滚，槽是白留的
JSON.stringify({ deClientW: document.documentElement.clientWidth,
                 bodyW: document.body.offsetWidth,
                 docScrollH: document.documentElement.scrollHeight,
                 docClientH: document.documentElement.clientHeight })
```

修法是 `html:has([data-swarmdrop-app]) { scrollbar-gutter: auto }`——用 `:has()` 而不是在
客户端 effect 里给 `<html>` 加类，后者会在首帧闪一下 15px。真正的滚动发生在 `PageShell`
内部那个 `overflow-y-auto` 里，它的滚动条本来就贴着内容区右缘。

**滚动条外观不用装包**：`scrollbar-width: thin` + `scrollbar-color`（Chrome 121+ / Firefox）
加 `::-webkit-scrollbar` 伪元素兜底旧 Safari，值与桌面 `src/index.css` 同源。
**作用域必须限定在 `[data-swarmdrop-app]`**，别用 `*`——文档站是 fumadocs 的观感，
不该被应用区的选择顺手改掉（同 base 层那条默认边框色规则的分寸）。
注意两套写法互斥：设了标准属性后 Chromium 会忽略 `::-webkit-scrollbar`，所以伪元素那版
只对旧 Safari 生效，两边都留是兜底不是叠加。

## 移动优先 + 920 断点，与桌面同一个数

应用区的基线视口是**手机浏览器**：单栏、无 hover 依赖、触摸目标 ≥44×44 CSS px。宽屏是渐进增强。

`(min-width: 920px)` 是全应用唯一的主从断点（`_lib/use-media-query.ts` 的 `MASTER_DETAIL_QUERY`），
**与桌面 `src/hooks/use-media-query.ts` 的同名常量是同一个数**。理由是 Windows 常见的 125% 缩放下
1200 物理像素只有 960 CSS 宽——正好落在 920 与 1024 之间，用 `lg:`(1024) 会让同一台机器上
桌面版分栏、Web 版堆叠。

### 页宽全站一个数，行长控制归文字（2026-08-06）

`PageShell` 此前按「这一页装的是什么」分三档（board 1240 / settings 1040 / form 860）。
分档的出发点没错，但**三档都 `mx-auto` 居中**，于是内容左缘随路由跳——1440 视口实测
设备/收件箱/传输在 224、设置 307、发送 402，**最远 178px**，而这几个入口在侧栏里挨着。

一个稳定的左缘对「这是一个应用」的观感比每页各自的理想行长更要紧。合并成一档之后，
行长换到它本来该在的层：**归文字自己**（`max-w-[860px]` 之类写在面板上），不再绑在页面容器上。

⚠️ 面板自己限宽时**不要 `mx-auto`**：页头满宽、面板居中会让两者左边缘对不齐，那正是
发送页此前的毛病（只不过当时是页面级 `column` 造成的）。**左对齐限宽**同时满足两条：
表单不铺满、左缘与页头和其它路由一致。

### 断点不止 920：导航侧栏占掉的宽度必须算进去（2026-08-06）

920 是**主从**断点（列表 ↔ 详情，两栏都是内容）。设备页那种「主内容 + 一栏辅助工具」不是主从，
而且**Web 应用区比桌面端多一条导航侧栏**——它有三档（≥1024 展开 224px · 768–1023 图标 64px ·
<768 底栏 0）。于是同一个视口宽度在两端剩下的内容宽并不一样：桌面设备页能在 920 就分栏，
正是因为那边没有侧栏。

照抄 920 的后果是主栏被压到装不下内容。设备页的实际账（`DEVICES_SPLIT_QUERY = 1280`）：

```
1280 − 224(侧栏) − 48(sm:px-6 两侧) = 1008 内容宽
1008 − 360(配对栏) − 32(栏间距)     =  616 主栏 → 正好两列设备卡（280×2 + 8）
```

再低一档（1024 视口）主栏只剩 376px，一列卡片配一条 360 的侧栏——两边宽度接近，
读起来是并列的两块而不是一主一辅。

**注意 `max-w-[1240px]` 是 border-box，含 padding。** 1600 视口下内容可用宽是
`1240 − 48 = 1192` 而不是 1240，主栏因此是 800 不是 848。算分栏时漏掉这 48px，
结论会差出小半列。

**新增布局断点时同时问两句**：① 这一页量的是主从还是主辅？② 导航侧栏在这一档占多少？

#### CSS 断点与 JS 断点必须是同一个数，且一起翻转

设备页的配对面板**默认是否展开由版式决定**（分栏时它独占一栏，收起只剩一栏空玻璃；竖排时
展开会把页面拉长半屏）。所以 `xl:` 栅格与 `useIsDevicesSplit()` 必须同时翻。

这也是**没用容器查询**的原因——容器查询更准（量的是内容宽而不是视口宽，天然免疫侧栏那三档），
但 JS 拿不到它的结果，折叠态就跟不上。用视口断点是在「准确」与「CSS/JS 一致」之间选了后者，
因为侧栏宽度本身是固定三档，视口断点在这里推得出来。

**相关文件**：`docs/app/app/_lib/use-media-query.ts`、`docs/app/app/_components/devices-section.tsx`

### ⚠️ `flex-1` 的 basis 是 0，所以它永远不会触发 `flex-wrap`（2026-08-06）

页头做成「标题组 + 右侧统计」并指望窄屏换行时踩的。写法是

```jsx
<header className="flex flex-wrap items-end justify-between">
  <div className="min-w-0 flex-1">…标题与描述…</div>
  <DeviceStats />
</header>
```

`flex-1` 展开是 `flex: 1 1 0%`——**basis 为 0 意味着这一项可以一直收缩到零宽**，于是
「子项理想宽度之和超过容器」这个换行条件永远不成立。375px 手机上的实际表现是描述被压成一条
5 行的窄柱、右边杵着三格统计，而不是统计换行到下一行。

修法是给它一个理想宽度：`flex-1 basis-64`（256px）。空间不够时才真的换行。

**判据**：只要 `flex-wrap` 的容器里有 `flex-1` 子项，就要问一句「它的 basis 是多少」。
要换行就必须给 basis，`min-w-0` 只管收缩下限，管不了换行。

### ⚠️ 但 `min-[920px]:` 不能和具名断点并列写——后者永远赢（2026-08-05 实测）

Tailwind v4 把**任意值断点 `min-[…]` 整族排在具名断点之前**。于是

```jsx
<ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 min-[920px]:grid-cols-3">
```

在 1440px 下两条规则都匹配，`sm:`(640) 排在后面赢——**设备网格从上线起就没有过三列**，
一直是两列。实测判据（在跑着的页面上 eval 即可复现）：

| 写法 | 视口 1440 下的 `gridTemplateColumns` |
|---|---|
| `min-[920px]:grid-cols-3` 单独 | `475px 475px 475px` ✅ 规则生成了也匹配 |
| `sm:grid-cols-2 min-[920px]:grid-cols-3` | `543px 543px` ❌ |

所以规则本身没问题，**并列写这两族才是错的**。三条出路，按优先级：

1. **不用断点。** 网格类布局用 `grid-cols-[repeat(auto-fill,minmax(280px,1fr))]`，
   让内容的最小可用宽度自己决定列数——设备网格现在就是这么写的。
   （`auto-fill` 不是 `auto-fit`：后者折叠空轨道，**只有一台设备时那张卡会横跨整行**。）
2. 全用具名断点（`sm:` / `lg:`）。
3. 全用任意值断点。

`src/`（桌面）里那几处是 `min-[920px]:` + `lg:`，顺序恰好就是想要的（920 先、1024 后覆盖），
**不受影响**——别顺手一起改。

顺带：就算那条 `min-[920px]:grid-cols-3` 生效了，结果也是错的——视口 920 时内容列约 608px，
三列每张 189px，而 Device Card Contract 要求八项信息位，189px 装不下。

`useMediaQuery` 用 `useSyncExternalStore` 且**服务端快照显式返回窄屏**：静态导出的预渲染 HTML
与客户端首帧因此一致，不会 hydration mismatch。

`_components/master-detail.tsx` 与桌面 `MasterDetailShell` 是**两份实现、同一套交互标准**——
桌面那份用玻璃拟态和为鼠标调的尺寸，搬过来要把整套玻璃 token 一起搬。

## Lingui 接 Next：SWC plugin 可用，但有三条硬约束

`@lingui/swc-plugin@6.6.0` 与 Next 16.2.6 的 `swc_core` ABI 兼容（宏编译 / `lingui extract` /
静态导出三件事都验过）。**升 Next 时要一起验**——不匹配的表现是构建期 panic 而不是一句清晰的
版本错误。

1. **源 locale 的目录静态 import 并在模块加载时同步激活**（`_lib/i18n.ts`）。预渲染发生在构建期，
   那一刻不能 await；不同步激活则预渲染出来的 HTML 是空壳。另两个 locale 按需动态 import，
   且**显式列成三条**而非拼模板字符串——后者会让打包器生成 context 模块。
2. **catalog 的 `.ts` 是产物不入库**，`.po` 才是事实源。`lingui compile --typescript` 挂在
   `postinstall` 与 `build` 两处，保证 IDE 与 CI 都拿得到。
3. **非组件模块只能定义描述符，不能展开**。`_lib/` 下的标签映射（`WEB_ERROR_KIND_LABEL`、
   `PHASE_META` 等）一律存 `msg\`\`` 描述符，由组件 `t(...)` 展开。同理，格式化函数**不许把 UI
   占位烤进返回值**——`formatTransferRate` 算不出来就返回 `null`，「等待数据」由调用点给。

### `metadata` 只能是源 locale，那是正确行为

`export const metadata` 在**构建期**求值，静态导出下没有「当前用户的 locale」。所以 `<title>`
走 `navTitle()` 取描述符的源文。运行时界面全部走 i18n。

### 连带的一个坑：client component 收不了带函数的 prop

`PageHeader` 因为要展开描述符而变成 client component，于是**不能再收整个导航项**——
`AppNavItem` 带一个 `icon` 函数组件，函数跨不了 RSC 边界，`next build` 在预渲染时直接报
`Functions cannot be passed directly to Client Components`。改成收一个 key，查表在客户端做。

**相关文件**：`docs/app/app/_lib/i18n.ts`、`docs/app/app/_components/i18n-provider.tsx`、
`docs/lingui.config.ts`

## 持久化偏好独立成 store，不塞进运行时 store

`_lib/store.ts` 是运行时节点状态（节点一关就该没了），`_lib/preferences-store.ts` 是本机设置
（关标签页也要留着，走 localStorage）。混在一起会让「什么该在刷新后还在」变成一道要逐字段
判断的题。目前偏好里只有设备组织（别名 + 分组）。

**它不会 hydration mismatch**：组织只影响已配对设备的渲染，而设备来自运行时节点——构建期一台
都没有，预渲染出来的必然是空态。**将来若有别的东西也读这份偏好、且它在预渲染时就有内容，
这条要重新考虑。**

解除配对时要 `preferencesActions.forgetDevice(peerId)`：别名与分组是本机偏好，内核不知道它们
存在，不清就会留下幽灵条目——同一个 PeerId 再次配对时还会顶着上一段关系的名字回来。

## 复制态的换代 key 必须挂在**持有状态的组件**上

上面「剪贴板」那条说了「给按钮加 `key={内容}`」，但有个前提容易漏：**key 只重置它所在的那个
组件**。如果复制态（`copied` / `state`）住在父组件里、key 挂在子 `<button>` 的 DOM 节点上，
换代什么也重置不了——按钮重新挂载，state 原封不动。

所以复制按钮要抽成自带状态的独立组件，key 挂在它身上：

```tsx
// ✅ state 与 key 在同一层
<CopyAddressButton key={details.remoteAddr} address={details.remoteAddr} />

// ❌ state 在父组件，key 白挂
<button key={details.remoteAddr} onClick={() => copy(details.remoteAddr)}>
```

具体到链路详情：连接从 relay 升级成 LAN 直连后 `remoteAddr` 会换，而按钮还挂着上一条的
「已复制」——用户照着粘出去的是一条已经不在用的地址。

复制逻辑本身已收口在 `_lib/use-copy.ts` 的 `useCopyToClipboard`（同步 TypeError、只有成功态
自动收回、连点顶掉旧 timer 三条取舍写在那里），两个调用点都用它。

**相关文件**：`docs/app/app/_lib/use-copy.ts`、`docs/app/app/_components/connection-badge.tsx`、
`docs/app/app/_components/invite-share.tsx`

## 本机跑双节点：`pnpm dev` 做不到，必须用静态产物起两个端口

传输、配对这些**要两个节点才能验**的功能，本机的搭法有两条硬约束，都实测踩过。
完整操作步骤（含本地 relay 怎么起）在
[`prompts/web-transfer-pause-resume-verify.md`](../prompts/web-transfer-pause-resume-verify.md)，
这里只记「为什么」与「症状长什么样」。

### 两个节点必须是两个 origin，而 `pnpm dev` 拦跨 origin

身份存 localStorage，同一 origin 的两个标签页共享它 —— 那是**同一个节点**，没法互相配对。
所以要两个 origin：静态导出产物 + 两个端口（`python3 -m http.server 3010 -d out` /
`3011`）最省事，都是 `localhost` 因而都满足 secure context（OPFS 与 WebCrypto 都要它）。

**不能改用 `pnpm dev` + `127.0.0.1` 凑第二个 origin**：Next dev server 会拦跨 origin 访问，
那一侧**页面根本不 hydrate**。症状极具误导性，且没有一条报错：

| 现象 | 为什么看起来正常 |
|---|---|
| UI 渲染完整 | 那是 SSR 出来的 HTML |
| 控件全 disabled | 服务端渲染时 `ready = false`，本就该 disabled |
| console 干净 | 客户端 JS 没跑，自然没有报错 |
| 节点停在「未启动」 | store 还是初始值，effect 一次都没执行 |

一眼可判的判据是 **wasm 有没有被 fetch**：

```js
performance.getEntriesByType("resource").filter((r) => /wasm/.test(r.name)).length
// 0 = 根本没 hydrate（不是 wasm 坏了）；1 = 正常
```

### 邀请带的是**全部** listen 地址，多一条公网 relay 就可能拨不通

`generate_invite` 把本机所有可达地址都编进去。若本机同时连着公网 relay 与本地 relay，
对端会先拨公网那条并失败——实测报的是
`Unexpected peer ID <relay 的 id> at <整条 circuit 地址>`，看起来像身份校验出了问题，
其实只是那条路不通。

本机验证时用 `NEXT_PUBLIC_SWARMDROP_WEB_RELAY_HELPERS` 把 helper 收敛成本地一条
（`_lib/relay-helpers.ts` 留了这个口子，**不必改源码**）。收敛之后**邀请串会明显变短**，
那是「地址列表干净了」的现成自检点。

## 拖文件进窗口的默认行为会把节点一起弄没，而只拦 `drop` 是无效的

浏览器对「把文件拖进窗口」的默认响应是**导航到那个文件**（地址栏变 `file:///…`），当前文档
连同它上面跑着的一切一起被销毁。对本应用这格外致命：没掉的不是「这次没加上文件」，而是
**正在跑的 P2P 节点**——连接断开、进行中的传输中止，回来还要重新 spawn。

而这恰恰是最容易发生的误操作：用户瞄准发送页那个虚线投放框、手抖偏两厘米就中。

两条只有踩过才知道的：

- **`dragover` 必须一起 `preventDefault`。** 只拦 `drop` 完全无效——不拦 `dragover`，浏览器
  压根不把窗口当成有效投放目标，`drop` 事件不会派发，默认导航照常发生。
- **拦下之后要改 `dropEffect`，否则等于换了种方式骗人。** 只 `preventDefault` 会让整个窗口
  显示「可以放」的光标，放下去却什么都没发生。窗口级监听里设 `dropEffect = "none"`，
  但要先判 `event.defaultPrevented`：真正的投放目标（发送卡片）已经处理过并设成了 `copy`，
  而窗口监听跑在冒泡最末端，无条件覆盖会把投放区的光标也改回禁止符。

护栏挂在 `app/app/layout.tsx`（`WindowDropGuard`），与两个入站请求宿主同样的理由：要在
**任何路由**下生效。文档站其它页面不受影响。

配套的一条：投放目标做成**整张发送卡片**而不只是虚线框（高亮仍只画在框上）。以及
`dragleave` 要判 `currentTarget.contains(relatedTarget)`——它会从子元素冒泡上来，不判的话
高亮会随鼠标经过每个子元素闪烁。

**相关文件**：`docs/app/app/_components/window-drop-guard.tsx`、`docs/app/app/_components/send-panel.tsx`

## 共享节拍的 hook：停表期间「现在」是冻住的，重新订阅要先拨表

`_lib/use-now-seconds.ts` 从「每个调用点各建一个 `setInterval`」改成了一个进程内共享的
`useSyncExternalStore`（相对时间进列表后同屏可以有几十个调用点，各自计时既是几十个定时器、
相位还各不相同——相邻两行会在不同时刻翻页）。

改造里唯一反直觉的地方：**最后一个订阅者走了要停表，而停表期间模块级的 `now` 就冻住了**。
「没人看」可以持续很久——用户在设备页待十分钟再进传输页，第一个订阅者拿到的是十分钟前的
「现在」，一屏的相对时间集体少算十分钟，还要等满一个节拍才自己纠正。所以 `subscribe` 里
必须**先拨表再开表**。（React 会在 subscribe 之后重新读一次快照，改完不用另行通知。）

`getServerSnapshot` 恒返回模块加载那一刻的值——它必须在一次渲染里稳定。当前没有调用点会进
预渲染产物（相对时间与邀请倒计时读的都是运行时数据，构建期一条都没有），所以不会
hydration mismatch；**将来若有构建期就有内容的调用点，这条要重新考虑**。

## 漏包 `<Trans>` 的裸中文，三道门禁一道都拦不住

`lingui extract` 只统计**被宏包住**的串——漏包的它根本看不见，于是「Missing 0」并不代表
没有漏翻。`tsc` / `next build` / `check:zustand-access` 更与文案无关。

实证：`transfer-activity-panel.tsx` 的「查看收到的文件」与 `receive-panel.tsx` 详情侧的
「已归档」徽标都是裸中文，混在满屏 `<Trans>` 里活了很久。后者尤其隐蔽——**同一个词在列表行
是包了的**，于是英文界面下列表显示 `Archived`、详情显示「已归档」，看起来像漏翻了一处翻译，
而不是漏包了一个宏。

**规矩**：新增 UI 串时自己扫一眼有没有裸中文（`aria-label` / `title` / `alt` 一并算），
别指望 extract 的统计数字。DESIGN.md 的跨端 UI 复查清单里那条「New user-facing strings go
through that build's i18n, including aria-label / title / alt」说的就是这件事。

## IndexedDB 的写读必须对称 —— 存字符串就得按字符串读

`idb::put_string` 存进去的是一个 **JSON 字符串**（`serde_json::to_string` 的结果），不是结构化
对象。读的时候必须 `value.as_string()` + `serde_json::from_str`；用
`serde_wasm_bindgen::from_value` 直接当对象读会**每一行都失败**：

```
invalid type: string "{\"capability_hash\":...}", expected struct StoredInvite
```

2026-08-03 在 `invite_store.rs` 实证到这个不对称，症状极其隐蔽：

- **写入是成功的**——IndexedDB 里躺着完整记录，用 devtools 或 `indexedDB.open()` 探得到；
- 只有读回来时静默丢弃，且丢弃走的是「单行坏了只丢这一行」这条**本来正确**的容错路径；
- 用户看到的是「已发出的邀请跨刷新全部消失」，于是也就无从撤销——而撤销是这个能力唯一的入口。

`inbox.rs` 的写读一直是对称的，可作对照模板。**新增任何一张 object store 时，把写与读放在
一起看一眼**：这类错配编译期查不出来，运行时也只在日志里留一行 warn。

**排查手法**（跨刷新丢数据这类问题通用）：先在浏览器 console 里数一遍 IndexedDB：

```js
(async () => {
  const dbs = await indexedDB.databases();
  // 逐个 open + objectStore(...).count()，看数据到底在不在
})()
```

数据在库里 = 写入没问题，问题在读路径；库里就没有 = 才去查写入。这一步能立刻把范围砍一半。

**相关文件**：`crates/web/src/invite_store.rs`、`crates/web/src/inbox.rs`、`crates/web/src/idb.rs`

## 「发送不跨刷新」是产品级约束，UI 要在三个层面表达它

浏览器上**发送方向的传输不跨页面刷新**，接收方向跨。这个不对称不是待补的缺口，是物理约束：
发送侧的文件内容来自用户选中的 `File` 对象，页面一刷新 JS 上下文销毁，浏览器不允许在用户未
重新选择的前提下再读同一个文件。所以非终态发送会话**连库都不落**（`crates/web/src/store.rs`
的 `worth_persisting`）——落了反而更糟，用户会看到一个点了必失败的「续传」按钮。

**2026-08-04 决策：不实现刷新后恢复发送。** 理论出路是 File System Access API——
`showOpenFilePicker()` 返回的 `FileSystemFileHandle` 可结构化克隆存进 IndexedDB，刷新后
`requestPermission()` 重新授权即可读回同一个文件。否决理由是它只有 Chromium 系支持
（Safari / Firefox 只有 OPFS 那部分），上了就得双路径 + 一句「为什么你的浏览器刷新后续不了」
的解释。要重新评估时先查这条支持面变了没有。

**同一页面生命周期内的暂停 / 续传是完整的**（2026-08-04 双 origin 实测：500 MB 文件在
26% 暂停、续传后跑到 100%，接收侧 sha256 与源文件逐字节一致），别把上面那条读成
「Web 端不支持暂停发送」。
`initiate_resume` 要的三样东西都在：会话记录（`create_session` 无条件写内存，
`worth_persisting` 只决定要不要**再**写 IndexedDB）、`File`（`OpfsFileAccess` 源注册表登记后
不移除）、bao outboard（`flow/send.rs` 在发送启动时就 `save_file_outboard`，Web 实现是写内存）。
连带一条：`build_sender_actor_for_resume` 里那段「outboard 缺失就按源文件重算」**在同页面暂停
场景根本不触发**——它有 `if pf.outboard.is_empty()` 守卫，而 outboard 一直在内存里。

UI 上由三件事共同表达，缺一不可：

| 层面 | 做法 | 为什么不能省 |
|---|---|---|
| 判据 | `_lib/format.ts` 的 `isLostOnReload`，与 `worth_persisting` 同源 | 手写 `direction === "send" && phase === …` 散在各处必然漂移 |
| 告知 | 传输详情侧提示条，判据是 `isLostOnReload` 而**不是** `phase === "suspended"` | 等暂停完再说就晚了——「待会儿再继续」的预期在点下那一刻已经形成，而传输中刷新同样整条丢 |
| 拦截 | `ReloadGuard` 挂 layout，有风险会话时注册 `beforeunload` | 面板那句话只有正看着会话时才可见，而关标签页可以发生在任何路由下 |

后两者是**互补不是重复**：浏览器早已不允许自定义 `beforeunload` 文案（一律显示厂商措辞），
所以拦截说不出原因，「为什么」只能由面板那句话讲；反过来面板那句话拦不住任何操作。

**发送页的「已发出」卡片刻意不重复这句话**：它的定位是「去向摘要 + 一条链接」，明细归传输页、
防损失归全局护栏。同一句话在两处说，就是窄屏空态那条约定反对的东西。

**相关文件**：`docs/app/app/_lib/format.ts`、`docs/app/app/_components/reload-guard.tsx`、
`docs/app/app/_components/transfer-activity-panel.tsx`、`crates/web/src/store.rs`

## 本机双节点：relay 的 `--external-ip` 不能是 127.0.0.1

`select_invite_addrs`（`crates/core/src/pairing/manager.rs:50`）**第一步就丢弃**
`is_loopback_or_unspecified()` 的地址。浏览器自己不 listen，它的可达地址全是
`/ip4/<relay-ip>/…/p2p-circuit/p2p/<自己>` —— 外层是 relay 的 IP。relay 挂在 loopback 上，
这些 circuit 地址就**整批**被过滤，邀请里一个地址都不剩。

症状极具误导性：两端 `relay_ready: true`、circuit 也确实建起来了、邀请也正常生成，只有对端
拨号时报 `Dial error: no addresses for peer`——看起来像 relay 或对端的问题，实际是本机邀请
从一开始就是空的。**一眼可判的自检点是邀请串长度**：带地址约 600 字符，不带只有 ~230。

用本机局域网 IP 起 relay（`--external-ip $(ipconfig getifaddr en0) --listen-ip 0.0.0.0`）即可。

**相关文件**：`crates/core/src/pairing/manager.rs`、`dev-notes/prompts/web-transfer-pause-resume-verify.md`

## 验证 `beforeunload` 要用合成事件，且探针会污染下一轮

自动化驱动（agent-browser / Playwright 一类）**默认自动接受 `beforeunload`**，所以看不到弹窗，
没法用「有没有弹框」判断拦截是否生效。改成断言事件是否被 `preventDefault`：

```js
const ev = new Event("beforeunload", { cancelable: true });
window.dispatchEvent(ev);
ev.defaultPrevented; // true = 有人拦了
```

两个坑：

1. **必须同一次页面加载内做对照**。`ReloadGuard` 只在存在非终态发送会话时才注册监听器，
   而会话状态一直在变——传输跑完转 terminal 之后返回 `false` 是**正确**行为。曾据此误判成
   「组件没生效」，实际只是测晚了几秒。正确做法是先测 baseline（仅终态 → `false`），再发一条
   会话后立刻复测（→ `true`）。
2. **自己注册的探针监听器会留在页面上**，之后每一轮 `defaultPrevented` 都被它污染成 `true`。
   `addEventListener` 传的是匿名函数就更摘不掉了。测完刷新页面再进行下一轮。

**相关文件**：`docs/app/app/_components/reload-guard.tsx`

## 应用外壳必须是**受限高度**，否则面板里的 `overflow-y-auto` 全是死代码

`app/app/layout.tsx` 的根是 `h-dvh` + `overflow-hidden`，`main` 是 `flex min-h-0 flex-1`
且**自己不滚**——滚动归页面，由 `PageShell` 的两个变体决定。

它曾经是 `min-h-screen` + 内容自然流。后果是**祖先链上没有任何确定高度的包含块**，于是
`transfer-activity-panel` 与 `receive-panel` 里写的 `min-h-0 + overflow-y-auto` 一行也没生效：
列表不会独立滚动，只会把整页撑长（滚列表时筛选条与操作按钮一起滚走）。这类失效**没有任何
报错**——CSS 不会告诉你 `overflow` 落在了一个高度不受限的盒子上。

排查手法：在 console 里数一遍页面上真正的滚动容器。

```js
[...document.querySelectorAll('*')]
  .filter(e => e.scrollHeight > e.clientHeight + 20 && getComputedStyle(e).overflowY !== 'visible')
  .map(e => e.className)
// 期望：只有一个，且是 PageShell 的那层；出现 <html>/<body> 就说明外壳没兜住
```

配套的两条：

- **`dvh` 不是 `svh`**：移动浏览器地址栏收起时可视高度会变，`dvh` 跟随它，`svh` 会让底部
  导航被顶出屏幕。
- **导航不再需要 `fixed` + 等高 spacer**。外壳受限之后，侧栏 / 顶栏 / 底栏都是 flex 里的
  `shrink-0` 子元素，那个「知道高度的人和补偿高度的人应当是同一个」的约定连同高度常量
  一起消失了——没有补偿，就没有失准的可能。

**相关文件**：`docs/app/app/layout.tsx`、`docs/app/app/_components/page-shell.tsx`

## 「还没加载出来」与「你什么都没有」必须分开判

`pairedDevices` / `projections` / `inboxItems` 的初值都是空集合，而 `startStatePoll` 要等
wasm 拉完 `_bg.wasm` 才第一次 tick。只看 `length === 0` 的空态，于是**每次刷新，老用户都先
看到一个带教学文案的确定性空态**——它在断言一件当时并不成立的事。

设备页、发送页、传输页三处都踩过（发送页那个 `devices.length === 0` 的早返回甚至跑在任何
`ready` 判断之前）。判据一律是 `status === "running"` 之后才谈「空」，此前给
`PanelSkeleton`。骨架而非 spinner：它保持内容的形状，切到真内容时不跳版。

**相关文件**：`docs/app/app/_components/empty-state.tsx`

## `setState` 之后同步读几何，读到的是**更新前**的那份（2026-08-06）

「点一下 → 展开某块 → 滚过去让它可见」这个组合有个陷阱：

```tsx
function open() {
  setExpanded(true);
  panelRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });  // ❌
}
```

`setExpanded` 是批处理的，状态要到事件处理器**返回之后**才 flush、DOM 才更新。所以
`scrollIntoView` 量到的是**收起态**的几何（一个只有标题行的外壳，约 100px）。

配合 `block: "nearest"` 就成了静默失效：规范说「元素已完全可见就什么都不做」，
而那 100px 的收起态**通常确实完全可见**——于是页面纹丝不动，展开出来的内容整个落在折线
以下。`behavior: "smooth"` 还把失准固化：滚动目标在调用时刻就算死了，之后元素长高不会
重新寻的。

**正确写法是把读几何推到下一帧**：

```tsx
setExpanded(true);
requestAnimationFrame(() => {
  panelRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  inputRef.current?.focus({ preventScroll: true });
});
```

### 配套的一条：目标本来就展开时，滚动不是反馈

同一个入口常常有两条来路，其中一条的目标**已经是展开的**（比如空态下配对面板默认就开）。
那条路上 `setExpanded(true)` 值没变、不重渲染，`scrollIntoView` 又因已可见而不滚——
两个动作都是 no-op，用户点了按钮屏幕毫无变化。

**聚焦目标里的第一个输入框**是唯一在所有来路上都成立的反馈：它有可见的焦点环，
对键盘用户还直接把光标放到了该打字的地方。`preventScroll: true` 让滚动仍由上面那次
`scrollIntoView` 统一负责，不叠加浏览器自己的聚焦滚动（两者节奏不同，叠起来会抖一下）。

**相关文件**：`docs/app/app/_components/pairing-panel.tsx` 的 `useImperativeHandle`

## 编错 CSS 变量名不会报错，只会静默把那条样式归零（2026-08-05）

写 `rounded-[var(--radius-card)]` / `p-[var(--space-card)]` 时**那两个变量并不存在**。
Tailwind 的任意值语法照样生成 `border-radius: var(--radius-card)`，浏览器解析不出来就丢掉
整条声明——圆角和内边距一起变 0，而 `tsc`、`next build`、`biome` 一个都不会响。
表现是「这张卡片看起来有点怪」，不是任何一条错误。

应用区实际有的只有这几个（`docs/app/global.css` 的 `@theme` 块）：

| 族 | 全集 |
|---|---|
| 圆角 | `--radius-panel-sm`(18px) · `--radius-panel`(24px)，外加 shadcn 的 `--radius{,-sm,-md,-lg,-xl}` |
| 间距 | `--space-in-group`(8) · `--space-in-panel`(16) · `--space-panel`(20) · `--space-section`(32) |

**没有 `--radius-card` / `--space-card`**：卡片级几何走 Tailwind 原生类（`rounded-xl` + `p-4`，
见 `device-card.tsx`），因为「面板 18–24px / 控件 6–14px」是 DESIGN.md 的两套词汇，
卡片属于后者、不需要单独一档 token。

**判据**：写 `var(--x)` 之前先 `grep -- '--x:' docs/app/global.css`。凭印象取名的变量名
（尤其 `--radius-card` 这种「听起来就该有」的）是最容易漏的一类，因为它读起来完全合理。

## 应用区不再跟 fumadocs 的 `--color-fd-*`

`app/global.css` 现在给应用区一套**自己的**无前缀语义 token（值与桌面 `src/index.css` 逐字
相同的 oklch 表达式），只有文档区仍读 `--color-fd-*`。两套名字互不相交，所以「文档区零影响」
的依据比从前更强——不是「我们小心地只新增别名」，而是**文档区根本不使用无前缀 utility**
（`app/`、`components/`、`content/` 下除 `app/app` 与 `components/ui` 外零命中）。

跟着文档皮肤走曾经带来三条量得出来的后果，都不是调一个数能解决的：
`muted-foreground` 在卡片内 4.20:1、焦点环 2.31:1、以及**卡片比背景还暗**（`#F1F1F1` on
`#F5F5F5`，看起来是凹陷的，正好撞上 PRODUCT.md 反面参照里的「灰上加灰」）。

一条容易漏的配套：**Radix 把 dialog / dropdown / popover / sheet / tooltip 的内容 portal 到
`document.body`**，落在 `[data-swarmdrop-app]` 作用域之外。`@layer base` 那条默认边框色规则
因此要额外覆盖 `[data-slot$="-content"]`，否则那些浮层拿的是 fumadocs 在
`css/lib/base.css` 里设的全局值——亮色下两者接近看不出，**暗色下差得很明显**。

**相关文件**：`docs/app/global.css`

## `role="progressbar"` 放进 `<button>` 等于没写

ARIA 对 `button` 规定 **Children Presentational: True**：它的后代角色会被辅助技术整个丢弃。
传输列表的每一行是个 `<button>`，里面那条进度条于是既不播报变化、也不被当成进度条——
而 `aria-label` 明明写着，维护者会以为它生效了。

`ProgressBar` 的 `label` 因此是 `string | null` 且**必填**：`null` 表示「我在一个可交互控件
内部」，组件退成 `aria-hidden` 的纯装饰，进度信息由按钮自己的可访问名承担（名字由后代文本
算出，百分比数字本来就在旁边那行里）。做成必填是刻意的——有默认值的话这个取舍就会被漏掉。

**相关文件**：`docs/app/app/_components/progress-bar.tsx`

## `PageShell variant="fill"` 之上的兄弟块必须自己限高（2026-08-04）

`fill` 变体给主从布局提供确定高度，自己**不滚**。收件箱页在主从之上还挂着
`IncomingOffersPanel`（待处理请求），它的高度随请求条数增长。请求一多：

- 主从被压到接近 0（`min-h-0` 允许压到 0）
- 「已收到的文件」整块够不着，而页面上**没有任何滚动条**提示还有东西

两道一起才够：

1. **请求区自己限高自滚** —— `max-h-[min(38dvh,340px)] overflow-y-auto`
2. **`fill` 外壳留可滚兜底** —— 外层 `overflow-y-auto`，内层
   `h-full min-h-[560px]`。`h-full` 而非 `flex-1`：两者正常视口下等价，但 `h-full` 是
   确定高度，极矮视口时内容可以超出它由外层兜住

**不要做**：把 `fill` 直接改成 `scroll`。那会让主从两栏失去确定高度，列内的
`overflow-y-auto` 重新变成死代码（同本文件「应用外壳必须是受限高度」那条）。

**相关文件**：`docs/app/app/_components/page-shell.tsx`、`incoming-offers-panel.tsx`

## 异步反查的结果要连「它属于谁」一起存

`InboxItemLink` 用 sessionId 反查收件箱条目。只存反查结果时，切到另一个会话后旧结果会
在新反查回来之前继续渲染——那条链接指向的是**上一个会话**的条目，点下去打开的是另一批
文件。effect 会重跑，但 state 不会自己回到「还没查」；节点未就绪时 effect 更是直接早退，
旧链接能一直挂着。

**正确做法**：`useState<{ sessionId, target } | null>` + 渲染期比对
`resolved?.sessionId === sessionId`。

**不要做**：在 effect 里先 `setTarget(undefined)` 清空——那对 `phase` 变化也会清一次，
链接会闪一下再回来。

**相关文件**：`docs/app/app/_components/transfer-detail.tsx`

## `prefers-reduced-transparency` 降级：没有 border 的玻璃面会整个消失

四个玻璃类里 `.glass-panel` 是**唯一没有 border** 的——它靠 blur + 半透明背景与页面分开。
降级块把 `background` 换成 `var(--card)`、`backdrop-filter: none` 之后，它就成了一块
与页面同色、无边、无阴影的矩形，面板边界整个消失。另外三个（card / control / accent）
自带 border，不受影响。

降级块里要单独给它补 `border: 1px solid var(--border)`。

**相关文件**：`docs/app/global.css`

## 结构化的业务结果不能压成 `WebError`（2026-08-05 修复）

`connect_invite` 曾在对端拒绝时返回 `WebError::network("邀请方拒绝了配对或配对未成功")`，
于是 `WebErrorCard` 渲染出标题「网络错误」加一行 mono 字体的**简体中文** —— 那句来自 Rust、
不在任何 `.po` 里，**en / zh-TW 用户看到的就是中文**；而真实原因是对方点了拒绝，
一个网络完全正常的场景。桌面（`src/stores/pairing-store.ts` 的 `getPairingRefuseMessage`）
与移动端一直是按判别码出 Lingui 文案的，只有 Web 把它当错误抛。

**判据：内核已经用结构化类型表达的结果，宿主层不许压成「某个错误 kind + 一句写死的自然语言」。**
那一步同时丢掉两样东西 —— 判别信息（reason 是什么）与本地化能力（那句话钉死在一种语言上），
而且往往落到语义相反的 kind 上。同形的还有 `OfferResult { accepted: false, reason }`。

现在 `PairingOutcomeJson` 带 `refused: PairingRefusedJson | null`，拒绝时返回 `Ok`，
前端查 `PAIRING_REFUSED_LABEL`。

**`crates/web/src/types.rs` 里的判别码只能是本地投影，不能直接用内核类型** ——
`swarmdrop-core` 在 `crates/web` 是 **wasm-only 依赖**（Cargo.toml 的
`[target.'cfg(target_family = "wasm")'.dependencies]`），而 `types.rs` **native 也要编**
（specta 导出跑在 native）。直接引用会报 `unresolved module swarmdrop_core`，
且只在跑 specta 导出时才暴露，wasm check 是绿的。

重复的安全性由**唯一构造点的穷尽 match** 保证：`node.rs` 里
`match response { PairingResponse::Refused { reason: PairingRefuseReason::UserRejected } => … }`
—— 内核加一个拒绝原因，那里编译失败。**别写成 `Option<String>` 的判别码**，
那只会在运行时静默落到兜底分支。

**加了新类型记得三步**：`types.rs` 定义 → `lib.rs` 导出（specta 导出 test 从 crate root 取）
→ `tests/specta_export.rs` 的 `register::<T>()`。嵌套类型会被自动带出来，但顶层的必须手动注册。
最后 `pnpm build:wasm` 重新生成 `packages/swarmdrop-web/*.d.ts`，否则 `docs` 的 tsc 会说
「Module 'swarmdrop-web' has no exported member」。

**相关文件**：`crates/web/src/types.rs`、`crates/web/src/node.rs`、
`docs/app/app/_lib/view-types.ts`（`PAIRING_REFUSED_LABEL`）、
`docs/app/app/_components/pairing-panel.tsx`

## 文件浏览器：三端共用 `@swarmdrop/file-browser`，取数走 adapter 不做形状嗅探（2026-08-06）

Web 的三处文件清单（传输详情 / 收件箱详情 / 发送面板）以及入站 offer 对话框，现在都用
`packages/file-browser` 的 `<FileBrowser>`（树形 + 网格），与桌面同一份组件。取数一律经
`_lib/file-browser-adapters.ts` 转成 `FileBrowserItem[]`。

**为什么必须走 adapter**：传输详情此前写的是 `live?.files ?? projection.files`——进度与投影
**二选一**。于是同一份数据有两种形状（`FileProgressInfo.transferred` vs
`TransferProjectionFile.transferredBytes`），渲染点得靠 `"transferred" in file` 现场嗅探；
更糟的是行的**身份与数量**在两种形状下由不同的东西决定，而进度域是按 sessionId 常驻的，
切换会话那一瞬取到的可能是另一条会话的采样。

现在的不变量在 `@swarmdrop/shared-view` 的 `fromProjectionFiles` 里，有回归测试钉着：

1. **投影是骨架，progress 只是覆盖层**。条目的身份/数量/名称/大小/路径永远来自 projection，
   progress 只按 `fileId` 覆盖「传了多少」与「什么状态」。
2. **终态忽略进度**，判定收在函数内部，不靠调用方自觉带上 `transferSample` 的 `live`。

`transferSample` 仍在，但只管**会话级**字节与百分比了。

**取图源两条分支**（`_lib/thumbnail-source.ts`）。`ThumbnailResolver` 收的是
`previewSource` **字符串**、返回 `Blob`——不是整个 item（item 每秒都在重建，传它会逼 hook
再养一个 ref 去躲开依赖），也不是 URL（管线第一步 `createImageBitmap` 只吃 `Blob`）：

- 收件箱 → `previewSource` 是 `file.relativePath`（OPFS 的键，与 `download_url` 同一个字段。
  **不要用 `localPath`**——Web 上它是带 `opfs:/` 前缀的展示值）→ `node.open_file()` 拿 `File`。
- 发送侧 → 待发文件的字节只活在内存里的 `File` 句柄上，**没有任何路径指得到它**。
  所以 `previewSource` 存的是自增序号，由 `createPendingFileThumbnailSource()` 按它回查。
  那个工厂**不收 `files` 参数**：收了的话每加一个文件就产出新 resolver 引用，而它是
  `useThumbnail` 的 effect 依赖——于是每加一个文件，已渲染的每张卡片都要重跑一遍取图。
  现在 resolver 引用恒定，变的是它内部那份 ref（调用点每次渲染 `setFiles(files)`）。

**桌面根本不走这条路**：它给的是 `previewUrl`（`convertFileSrc` 的 asset URL，直接能渲染）。
`previewUrl` 与 `previewSource` 是 `FileBrowserItem` 上**两个不同的字段**，不是两个名字——
合成一个的话，`FileCard` 只能靠「调用方有没有传取图源函数」反推自己拿到的是哪一种。

**非 secure origin 提前判掉**（`detectSecureContext()`，且**只算一次**——那三个属性在一个
文档的生命周期内不会变），不要让 `open_file` 去报错：那条路径每张图都要付一次 5s 超时的等待。

**相关文件**：`docs/app/app/_lib/file-browser-adapters.ts`、`docs/app/app/_lib/thumbnail-source.ts`、
`packages/shared-view/src/file-browser/adapters.ts`、`packages/file-browser/README.md`

## 改了 `packages/file-browser` 就必须在 `docs/` 重装（2026-08-06 实证）

`docs` 用 `file:` 协议引它（不能用 `link:`，理由见 `toolchain.md` 的实例分裂那条），而 pnpm
对 `file:` 目录依赖用**硬链接**——硬链接理论上共享 inode，但编辑器/工具几乎都是写临时文件再
`rename()` 覆盖，新文件是新 inode，链接当场断。

**症状会伪装成别的问题**：新增文件 → Next 报 `Module not found`；改了已有文件 → `docs` 的
`tsc` 报「某属性不存在于类型上」，而源文件里明明有。两种都是先 `cd docs && pnpm install`
（几秒），再怀疑代码。

**相关文件**：`docs/package.json`、`packages/file-browser/README.md`
