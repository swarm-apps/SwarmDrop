# Web 应用区成熟度差距分析（2026-08-04）

> **状态**：✅ **已实施**（同日）。本文件保留为**诊断记录**——它记的是「当时为什么是坏的」，
> 而不是待办清单。落地后的规则去处：
>
> - 跨端主题统一规则 → `DESIGN.md` §2「Cross-platform token unification」与 State Ink Rule
> - 跨端契约的修订（信任/连接徽标并存、offer 关闭语义的两条配套） → `DESIGN.md` 对应各节
> - Web 表现层的坑（受限高度外壳、假空态、token 层、progressbar-in-button）
>   → `dev-notes/knowledge/web-app-frontend.md`
> - 路径穿越与 Web 源 id → `dev-notes/knowledge/rust-backend.md`
>
> 唯一**未做**的一项是「Web 端搬 WebGL ambient 背景」——决策为不搬，理由记在
> `DESIGN.md` 的允许分叉清单里。
> **范围**：`docs/app/app`（Web 应用区）对照 `src/routes/_app`（桌面）与 `mobile/`（移动）。
> **判据**：`DESIGN.md` 三份跨端契约 + 跨端 UI 复查清单；`PRODUCT.md` 设计原则与 WCAG AA；
> impeccable 的 product register。
> **方法**：三个并行 agent 逐模块对读 + 主线自查主题/结构/无障碍，结论互相交叉验证。
> 所有结论带 `文件:行号`。

## 结论摘要

Web 端**不是「功能没做完」**——wasm 25 个公开方法里 24 个前端已接。问题分三层：

1. **两个 P0 正确性缺陷**（一个 Web、一个桌面），与 UI 打磨无关，必须单独修。
2. **缺一层应用外壳 + 主题接错了源**——结构性的，导致每页都是「薄卡 + 大片空白」。
3. **一批「有能力没接线」**——分组删不掉、relay 状态不显示、传输历史截断且无筛选。

同时查明：**Web 端在 11 处反而比桌面端做得对**（信任徽标、自我邀请过滤、队列剩余数、
会话 ID、连接方式呈现、offer 关闭语义…），桌面端有 6 条自己的缺口。
这不是「Web 追桌面」，是**双向对齐**。

---

## 零、两个 P0 正确性缺陷

### P0-1（Web）同名文件会互相覆盖，可能发出错内容

`crates/web/src/node.rs:794` 把源 id 写成文件名：`FileSourceId(file.name())`，
而 `crates/web/src/file_access.rs:59` 的 `OpfsFileAccess.sources` 是
`HashMap<FileSourceId, File>`。

后果链：一次发送里挑两个同名文件（不同目录下的 `report.pdf` 很常见）→ 后者覆盖前者 →
两条 entry 指向同一个 `File`，但各自声明的 size 不同 →
`crates/transfer/src/flow/prepare.rs:56-63` 要么报「read_source_chunk 长度异常」失败，
要么**发出错误内容**。全链路无去重。

桌面不会撞：源 id 是绝对路径。

### P0-2（桌面）入站文件 offer 关闭即拒绝，且几乎无法不拒绝地退出

`src/components/transfer/transfer-offer-dialog.tsx:128-135` 的 `handleOpenChange(false)`
直接调 `handleReject()`；`:144` 的 `showCloseButton={false}` 与 `:145` 的
`onPointerDownOutside` 拦截堵死另外两条出口——**唯一退出是 Esc，按下去作废对方整次传输**。

DESIGN.md:283-285 明写 file offer 的关闭 **≠** 拒绝（"a mis-tap costing someone a whole
transfer is far worse than one that costs a second click"）。DESIGN.md:289-290 只承认了
「桌面没有回看入口」，**没有承认这一条**——实际状况比文档记载的更严重。

它同时是强制模态：收到 offer 时桌面用户做不了任何别的事。

---

## 一、主题：三端实际上是三套

三份 token 换算到同一色彩空间后比对：

| Token | 桌面 `src/index.css` | Web 实际生效（fumadocs neutral） | 移动 `mobile/src/global.css` |
|---|---|---|---|
| 主色填充 | `oklch(.583 .105 177.1)` `#0F8F7A` | ✅ 同桌面（`--brand-solid`） | **`oklch(.516 .093 178.2)` `#087968`** |
| 填充上的字 | 深墨 `#020817`（4.98:1） | ✅ 同桌面 | **纯白**（5.32:1） |
| 亮色底 | `#FFF`（外壳 `#FBFCFC`） | **`#F5F5F5`** | `#FFF` |
| 亮色卡 | `#FFF` | **`#F1F1F1`（比底还暗）** | `#F5FAF8`（带绿） |
| 暗色底 | `oklch(.18 .01 260)` 冷藏蓝 | **`#121212` 纯中性黑** | **`#121E20` 青蓝** |
| 暗色卡 | `oklch(.27 .008 260)` | `#191919` | `#18282B` |
| 焦点环 | 品牌青绿 | **中性灰 `#A3A3A3`** | 品牌青绿 |
| 语义状态色 | 只有 `--destructive` | 只有 `--destructive`（**定义了没人用**） | success/warning/destructive **+ ink 变体** |
| 面板圆角 | `24px`/`20px` 两档词汇 | **全部塌成 `rounded-lg`×50 + `rounded-xl`×17，无 18–24px** | — |

### 1.1 移动端把「文字形态的青绿」当成了「填充形态」

`#087968` 正是桌面 `--brand`（文字色），不是 `--primary`（填充 `#0F8F7A`）。
DESIGN.md 的 Brand Fidelity Rule 明写这个双 token 拆分 load-bearing
（"never use the fill teal as small text on white"），但**反向误用没被拦住**。

后果：同一颗「发送」按钮，手机上比桌面暗一档，字一边白一边深墨。

### 1.2 暗色三个不同色相

冷藏蓝（hue 260）/ 纯中性（无 hue）/ 青蓝（hue 209）。DESIGN.md 写的是
"The cool navy-tinted dark neutrals **deliberately** stay"——Web 与移动都没遵守，
且各自跑向不同方向。

### 1.3 三条可量化的无障碍缺陷（Web）

| 项 | Web 实测 | 桌面 | 标准 | 判定 |
|---|---|---|---|---|
| `muted-foreground` 在卡片内 | **4.20:1** | 4.74:1 | WCAG AA 4.5:1 | ❌ |
| 键盘焦点环 | **2.31:1** | 4.02:1 | WCAG 2.2 SC 1.4.11 → 3:1 | ❌ |
| 卡片 vs 背景分层 | **1.04:1** | 靠边框+阴影 | 需可辨 | ❌ |

根因同一个：`docs/app/global.css` 的 `@theme inline` 把 shadcn 语义 token 映射到
`--color-fd-*`（fumadocs 的**文档**皮肤），只有 `primary` 接了品牌色。文档皮肤服务长文阅读，
不是应用界面。

### 1.4 「在线」绿字三端都不达标（亮色）

| | 实测 |
|---|---|
| 桌面 `text-green-500` on 玻璃卡 | **2.28:1** ❌ |
| Web `text-emerald-600` on 卡片 | **3.34:1** ❌ |
| Web `text-emerald-600` on 白 | **3.77:1** ❌ |
| `text-amber-600` on 白（拒绝原因等三处） | **3.87:1** ❌ |

移动端为此专门做了 `--success-ink` / `--warning-ink` / `--destructive-ink` 并写明了理由
（"状态色本身当文字都低于 WCAG AA，amber 尤甚 ~2:1"）。**这个修正只有移动端做了。**
暗色下三端都没问题（8–9:1）。

### 1.5 词汇混用

应用区 **70 处**直接写 `bg-fd-*`/`text-fd-*`/`border-fd-*`（9 个组件），与写
`bg-card`/`text-muted-foreground` 的组件混在一起；当前两套碰巧同值，所以**不可见**。
另有 8 处裸 `red-*`、若干 `emerald-*`/`amber-*` 绕过 `--destructive`。

---

## 二、结构：缺一层应用外壳

### 2.1 没有受限高度的包含块 → 页内滚动全部失效

`docs/app/app/layout.tsx:40` 是 `min-h-screen`（不是 `h-screen`），`main` 是 `flex-1`。
于是 `transfer-activity-panel.tsx:348` 与 `receive-panel.tsx:441` 写的
`min-h-0 + overflow-y-auto` **是死代码**——祖先链上没有确定高度。

桌面是 `_app.tsx:83` 的 `h-svh flex flex-col` + `main flex-1 overflow-hidden`，
`master-detail-shell.tsx:131` 再 `grid h-full … overflow-hidden`，列表与详情各自滚。

### 2.2 `max-w-4xl` 与 920px 主从断点互相打架

`layout.tsx:53` 限宽 **896px**，而 `MASTER_DETAIL_QUERY` 是 **920px**。
1920px 显示器上双栏永远挤在 896px，两侧各空 400px。桌面同场景 `max-w-[1240px]`。

### 2.3 玻璃层与 ambient 背景不存在

DESIGN.md 称其为 "the system's single biggest personality investment"，
Web 端零对应物（`.glass-panel`/`.glass-card`/`.glass-control`/`.glass-accent` +
`--app-shell-background` + `prefers-reduced-transparency` 降级）。

### 2.4 三处「假空态」：把「节点没起来」说成「你什么都没有」

| 位置 | 判据 | 后果 |
|---|---|---|
| `device-grid.tsx:59` | 只看 `rows.length` | 每次刷新，老用户先看到「还没有已配对的设备」+ 教学文案 |
| `send-panel.tsx:135` | `devices.length === 0` 早返回，跑在 `ready` 判断之前 | 启动期显示「还没有可发送的设备 / 去配对」 |
| `transfer-activity-panel.tsx:343` | 同上 | 同上 |

`pairedDevices` 初值 `[]`（`_lib/store.ts:188`），`startStatePoll` 要等 wasm 拉完
`_bg.wasm` 才首次 tick（`_lib/state-poll.ts:38`）。桌面用 `isOnline` 分流到
`OfflineEmptyState`（`index.lazy.tsx:319`）。**全应用无骨架屏**，`PanelFallback` 只覆盖 Suspense。

### 2.5 空态系统性偏弱

发送/传输/收件箱/设备四页空态**都没有主 CTA 按钮**，只有内联文字链接；
没有 `CenteredEmptyState` 那样的圆形图标徽章。设备页空态还把页头那句话原样又说了一遍。

### 2.6 侧栏与内容区无分层

`app-nav.tsx:85` 的 `bg-fd-card/40` 压在 `#F5F5F5` 上 ≈ `#F3F3F3`。
product register 要求 "A second neutral layer for sidebars, toolbars, and panels"。

### 2.7 导航断点（768/1024）与主从断点（920）互不对齐

`nav.ts` 注释写「920 是全应用区唯一的主从断点，整个应用区在同一断点一起换形态」，
但导航自己用 `md:`(768) 与 `lg:`(1024)。768–920 与 920–1024 两个区间里各换各的。

---

## 三、功能面差距

### 3.1 设备 + 配对

| 功能点 | 桌面 | Web | 缺口 | 严重度 |
|---|---|---|---|---|
| **分组重命名 / 删除 / 排序** | 完整管理面（`device-organization-dialogs.tsx:238-405`） | **`renameGroup`/`deleteGroup` 零调用点**（`preferences-store.ts:90,103`），无「管理分组」入口，无 `reorderGroups` | Web 前端 | **P1** |
| **按分组过滤设备** | `index.lazy.tsx:147-156,468-501` | 无 | Web 前端 | P1 |
| 区块标题 + 计数 | `SectionHeader title count` | 直接 `<ul>`，无标题无计数 | Web 前端 | P2 |
| **「撤销没保存」告警** | `toast.warning`，脱离列表存活 | **渲染在 `{invites.length > 0}` 块内**（`pairing-panel.tsx:485,525-529`）→ 撤销最后一条后整块卸载，告警永远看不到 | Web 前端 | **P1（真 bug）** |
| 「仅局域网可见」控件 | `Switch` + 独立说明 + 后果说明 | **裸 checkbox ≈13-16px**（`pairing-panel.tsx:450-458`），全页最小可点目标，而它改变邀请的安全边界 | Web 前端 | **P1** |
| 链路详情触发器触摸目标 | 指针设备无要求 | **≈22px**（`connection-badge.tsx:53-60`），移动端唯一入口 | Web 前端 | **P1** |
| 剪贴板邀请感知 | 全局（`_app.tsx:93`） | 只在 `/app/devices` 挂载（`pairing-panel.tsx:294-314`） | Web 前端 | P1 |
| 配对文案 | 产品化 | 开发者调试文案（"需先在设置页的「连接」区建立可达 (circuit)"…） | Web 前端 | P1 |
| 邀请倒计时临期告警 | 30s 转 amber（`generate.lazy.tsx:41,198-201`） | 一直 muted 到 0 再跳覆盖层 | Web 前端 | P2 |
| 二维码生成期 | 定位角骨架 | 一行文字 | Web 前端 | P2 |
| 入站配对请求的设备图标/平台 | 有（`connection-request-dialog.tsx:70-78`） | 无——`PendingPairingJson`（`crates/web/src/types.rs:107-111`）只有 3 字段，`event_bus.rs:49-59` 拿着 `os_info` 只调 `.display_name()` 就丢弃 | **wasm**（唯一一条） | P1 |

**Web 反而更优（6 条）**：信任徽标恒在场 · `canSendToDevice` 判据 · 44px 触摸目标 ·
解除配对失败就地报错不关框 · 自我邀请/已配对过滤收口在唯一入口 · 邀请失效四态
（`consumed`/`expired`/`revoked`/`unreachable`，桌面只有 4 个通用态且缺 consumed/revoked）。

### 3.2 发送

| 功能点 | 桌面 | Web | 缺口 | 严重度 |
|---|---|---|---|---|
| 选目录 | `pickFolderAsSource()` + 后端递归枚举 | 无 `webkitdirectory`，且 `node.rs:794-802` 把 `relative_path` 写死成 `file.name()`——**即使前端补了，路径结构也会被拍平** | **两者** | P1 |
| 无效深链目标的说明 | 明确空态「设备未找到」+ 返回 | **一句话都不说**：守卫是 `target && !targetValid`（`send-panel.tsx:481`），而已解除配对时 `target === null`，条件不成立 | Web 前端 | P1（违反 Send Entry Contract 第 4 条） |
| 发送后就地看进度 | `/send` 就地切 `SendProgressView`，含完整暂停/取消/重发 | `SentSessionCard` 只有状态文字 + 一条链接——**刚发出的传输在发送页取消不了** | Web 前端 | P1 |
| 失败后重新发送 | `canResendProjection` → share-target | 无，且**没有替代出口**（浏览器读不回 `File` 是物理约束，但「重新选文件发给同一台」这条链接可以给） | Web 前端 | P1 |
| 目标选择器 | shadcn `Select` | **裸 `<select>`**（`send-panel.tsx:464`） | Web 前端 | P1 |
| 清空全部已选 | 无按钮（`clear()` 只被「发送更多」调用） | 无 | 两端 | P2 |
| 剪贴板粘贴文件 | 无 | 无 | 两端 | P2 |

**Web 反而更优**：投放目标是整张卡片 + `WindowDropGuard` 全局护栏（桌面无窗口级误投护栏）；
多文件批次计数用独立 `shrink-0` 徽标（桌面的「等 N 个文件」会被 truncate 吃掉）。

### 3.3 传输

| 功能点 | 桌面 | Web | 缺口 | 严重度 |
|---|---|---|---|---|
| **历史列表完整性** | 全量渲染无上限 | **硬截断 8 条**（`transfer-activity-panel.tsx:59,119`），第 9 条起 UI 不可达，只有 `?session=` 深链能捞 | Web 前端 | **P1** |
| **筛选** | 四个筛选器 + 计数，`?filter=` 进 URL | **完全没有**，只有「进行中/最近完成」两组 | Web 前端 | **P1**（与上条叠加） |
| 暂停/续传行内入口 | 详情 + 列表行双入口 | 仅详情侧 | Web 前端 | P2 |
| 入站 offer 来源徽标（MCP/策略） | `origin.type === "mcp"` 徽标 + `PolicyReasonBadge` | 无——`IncomingOffer` 在 store 边界就把 `origin`/`policyAction` 丢了（`_lib/store.ts:44-60,462-470`），**wasm 事件本身带这些字段** | Web 前端 | P1（PRODUCT.md 原则 5） |
| 全局「暂停接收」 | 托盘可切，React UI 未接 | 无（域层 `set_receiving_paused` 已实现，wasm 未导出） | wasm | P2 |
| 校验信息（checksum） | 无 | 无 | 两端 | P2 |

**Web 反而更优**：连接方式呈现（桌面**零命中**，而 PRODUCT.md:41 把它列为第二条设计原则）·
会话 ID mono + 复制 · offer 队列剩余数 + `aria-live` · offer 关闭 ≠ 拒绝 ·
「发送不跨刷新」三层完整表达。

### 3.4 收件箱

| 功能点 | 桌面 | Web | 缺口 | 严重度 |
|---|---|---|---|---|
| 时间分组（今天/昨天/本周/更早）+ 吸顶组头 | 有，日历运算避 DST | 无，平铺 `<ul>` | Web 前端 | 中 |
| 键盘 ↑/↓ 选条目 | 有（含 focus + scrollIntoView） | 无 | Web 前端 | 中 |
| 详情：来源类型 / 内容类型 / 关联传输状态 | 三项都有 | **三项都无**（DTO 里全都有） | Web 前端 | 中 |
| 详情 → 传输记录 | 「打开传输记录」按钮 | 无（反向链路存在，**链路单向**） | Web 前端 | 中 |
| 搜索片段高亮 | `<mark>` | 只渲染 snippet 不高亮 | Web 前端 | 低 |
| 文件类型图标 / 文件浏览视图 | `getFileIcon` + `FileBrowser` 网格/树 + 缩略图 | 一条 `<ul>`：名 + 大小 + 下载 | Web 前端 | 中 |
| 骨架屏 | 三套 + keep-previous 防闪 | 无 | Web 前端 | 低 |
| **已读/未读** | **无任何视觉**——`open_inbox_item` 内部写 `mark_inbox_item_opened`（`inbox.rs:85`），但 `lastOpenedAt` 在 `src/` **零引用** | 未读点 + 粗体 + 下载即标已读（带并发去重） | **桌面前端** | **高（桌面欠 Web）** |
| 批量操作 | 无 | 无 | 两端 + 内核 | 中 |
| 「另存为/导出到目录」 | 命令在但 **UI 零调用**（死代码） | 无（下载即导出） | 桌面前端 | 低 |
| 待处理 offer 回看 | **无回看入口**（DESIGN.md:290 已记） | 收件箱页回看列表 | 桌面前端 | 中 |

### 3.5 设置 —— Web 最大的功能缺口

| 项 | 桌面 | Web | 判定 |
|---|---|---|---|
| **信息架构** | 三级基元 `Section→Card→Row` + bento 网格五行 | **四面板平铺，零分组零基元**；四张卡标题权重完全相同，与 `h1` 只差 2px；最低频的语言排在最前 | **高** |
| **主题切换** | 三选一 + 迷你预览缩略图 | **完全没有**（全 `docs/app/app/` 无 `useTheme`）。唯一出口是侧栏 `/docs` 链接，而**离开 /app 会卸载节点、中断传输** | **高** |
| **bootstrap/relay 管理** | 只读默认清单 + 自定义增删 + Multiaddr 校验 + 传输徽标 + 重启横幅 | 单文本框；默认清单来自构建期环境变量用户改不了；`relays_state`/`relays_changed` **前端零调用**；`helperIdRef` 只记手动登记的 id，**撤销撤不掉自动登记的** | **高** |
| 关于 / 版本 / 外链 | 品牌卡 + 版本 + 官网/GitHub/更新日志 | **无关于区** | 中 |
| PeerId 复制 | 截断 + 点击复制 + `Check` 反馈 | 全量 mono，**不可复制**（`use-copy.ts` 已有） | 中 |
| 连接指标（已连节点/配对数/NAT） | 三格指标 | 无 | 中 |
| OPFS 用量与清理 | N/A | **无**——`inbox/page.tsx:16-18` 自述「入口将来开在这里」 | 中 |
| 已发出邀请的位置 | 设置页（`-sent-invites-section.tsx:5-9` 有推导） | 设备页配对面板里 | 中（位置分叉） |
| `DevEventLog` | N/A | 生产设置页第四张卡，**无 dev 门控**（自述「非主 UI 反馈」） | 中 |

**Web 本就不适用**：窗口关闭行为 · 自动启动 · 局域网协助节点 · 保存路径 · MCP Server ·
托盘 · updater · 打开文件/所在目录/复制本地路径 · repair · `missing` 标记。

---

## 四、反馈层：Web 端 0 处 toast

桌面 **76 处** `toast.*`（`sonner`）。Web 应用区 0 处，`docs/components/ui/` 无 `sonner`
（`use-copy.ts:10` 自述「应用区没有 toast 系统」）。

复制邀请、解除配对、撤销邀请、归档/取消归档、删除、清空历史、下载——**做完全部静默**。
DESIGN.md 跨端清单「Destructive actions require one explicit confirmation,
**and can report failure**」在 Web 端只兑现了前半句。

---

## 五、无障碍专项（Web）

| 问题 | 位置 | 后果 |
|---|---|---|
| **进度条被移出无障碍树** | `transfer-activity-panel.tsx:490` 把 `role="progressbar"` 放进 `<button>`（`:454-506`） | ARIA Children Presentational: True——读屏完全听不到进度 |
| 触摸目标 ≈24px | `confirm-action.tsx:47` 的 `INLINE_ACTION_CLASS` 是**暂停/续传/取消/删除/查看收到的文件**全部动作的皮肤 | 移动端主要操作难点中 |
| `role="radiogroup"` 无 roving tabindex | `trust-policy-dialog.tsx:182-201` | 四个 radio 各自可 Tab，不符 APG |
| 裸 button 无 `focus-visible` 环 | `pairing-panel.tsx`/`connection-panel.tsx`/`node-panel.tsx` 多处 | 键盘用户看不到焦点 |
| 裸英文串未包 `<Trans>` | `connection-panel.tsx:112` 的 `: "connect"` | `lingui extract` 拦不住 |

桌面侧对应：进度条四个调用点无一传 `aria-label`（`progress.tsx` 是 Radix Root，有 role
没有名）；`send/index.lazy.tsx:298` 漏包 `<Trans>`（同串在 `file-browser.tsx:62` 包了，
所以 `extract` 的 Missing 计数看不出来）。

---

## 六、代码结构腐化

| 文件 | 行数 | 症状 |
|---|---|---|
| `transfer-activity-panel.tsx` | 983 | 5 张标签表 + 4 个纯函数 + **8 个组件**；`InboxItemLink`(:942-983) 在传输面板里发起**收件箱**异步反查 |
| `receive-panel.tsx` | 783 | 两个不相关顶层导出（决策 vs 结果）；`InboxPanelInner`(:189-505) 单函数 316 行、9 个 state/ref、4 个 effect；动作对象在 JSX 里现构（:472-496，含 60 行行内注释） |
| `pairing-panel.tsx` | 538 | 27.3KB |
| `send-panel.tsx` | 500 | `SendPanelInner`(:71-300) 单组件 230 行 |

`_components/` 已有 34 个文件，**拆分成本很低**。

---

## 七、优先级

### P0 — 正确性
1. Web 同名文件覆盖（`node.rs` 源 id 去重）
2. 桌面 offer 关闭即拒绝 + 无法不拒绝退出

### P0 — 无障碍不合规
3. Web 应用区中性色阶改为自有（修 4.20 / 2.31 / 1.04 三条）
4. 焦点环改品牌色
5. 进度条移出 `<button>`
6. 三端「在线」绿字 + amber 文字引入 ink 变体

### P1 — 功能缺失
7. Web 设置页主题切换
8. Web 分组重命名/删除/管理入口
9. Web 传输页筛选 + 去掉 8 条硬截断
10. Web bootstrap/relay 可视化管理（接 `relays_state`/`relays_changed`）
11. Web 邀请「撤销没保存」告警脱离列表
12. Web 三处假空态 → 骨架/加载态
13. Web 无效深链目标说明
14. Web 发送页就地取消 + 失败后重发出口
15. 桌面收件箱已读态（`lastOpenedAt` 写了不读）
16. 桌面 device-card 信任/连接徽标三元；发送按钮认 `blocked`；手动粘贴过滤

### P1 — 成熟度
17. 应用外壳受限高度 + 放开 `max-w`
18. toast 反馈层
19. 空态统一（图标徽章 + 主 CTA）
20. 玻璃层 token + 面板圆角词汇
21. 设置页分组重构
22. 原生元素 → shadcn（select / checkbox / input / button 多处）
23. 触摸目标 44px 铺开
24. 配对文案产品化

### P2
25. `fd-*` 词汇统一 · 裸语义色收口 · 导航断点对齐 920
26. 收件箱时间分组、键盘导航、来源/内容类型、→传输记录链路
27. Web 选目录（需同改 `node.rs` 的 `relative_path`）
28. 大文件拆分
29. 移动端主色对齐（**需决策**）
30. 入站配对请求设备图标（**唯一 wasm DTO 缺口**，要重跑三条生成链）

## 八、待决策

见会话记录。
