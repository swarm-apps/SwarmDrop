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
| 3 | `install_upgrade_handler` 的 `for name in [KV_STORE, SESSION_STORE, INVITE_STORE, INBOX_STORE]` | 版本提了、回调也进了，但**不建这张表** |

2 与 3 都只在**运行时**暴露，且症状一模一样：第一次读写这张表拿一个 DOM 异常
（`NotFoundError: One of the specified object stores was not found`）。
`cargo check` / `check-wasm.sh` / `pnpm build` 全绿，跑起来才炸 —— 而这三样是平时唯一的门。

两条配套：

- **`onupgradeneeded` 里只做「建缺失的 store」，逐个判存在性、不按版本号分支。**
  老库升级与新库首建因此走同一段代码，不需要 `if old_version < 4` 这类阶梯。
  加新表时**不要**为它开特例。
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

### 品牌色与桌面同源，写成同一组 oklch 表达式

`--brand` / `--brand-solid` / `--brand-ink` 分别对应桌面 `src/index.css` 的
`--brand` / `--primary` / `--primary-foreground`。此前这边是 hex、那边是 oklch，实测**本来就是
同一组颜色**（最大通道差 0–1，取整误差）——但「同一组」这件事只能靠人去转换才看得出来。
现在两份文件的这几行可以直接肉眼比对，改一边漏另一边会显眼。

**相关文件**：`docs/components.json`、`docs/app/global.css`、`docs/lib/cn.ts`

## 移动优先 + 920 断点，与桌面同一个数

应用区的基线视口是**手机浏览器**：单栏、无 hover 依赖、触摸目标 ≥44×44 CSS px。宽屏是渐进增强。

`(min-width: 920px)` 是全应用唯一的主从断点（`_lib/use-media-query.ts` 的 `MASTER_DETAIL_QUERY`），
**与桌面 `src/hooks/use-media-query.ts` 的同名常量是同一个数**。理由是 Windows 常见的 125% 缩放下
1200 物理像素只有 960 CSS 宽——正好落在 920 与 1024 之间，用 `lg:`(1024) 会让同一台机器上
桌面版分栏、Web 版堆叠。设备网格也在这个宽度升到三列，整个应用区一起换形态。

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
