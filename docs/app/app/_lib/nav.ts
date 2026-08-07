// Web 应用区导航的单一事实源：侧边栏、底部导航、页头标题、各页 metadata 与**所有跨页链接**
// 都从这里派生。新增页面只改这一处。
//
// 「所有」是认真的——组件里手拼 `/app/xxx` 字面量会让这份事实源退化成注释：改一个路由段，
// 这里改完那些字面量静默失效，而静态导出没有死链检查。带参链接一律走下面的 builder。
//
// 分区（devices / send / inbox / transfer / settings）刻意对齐桌面端 `src/routes/_app/`，
// 但**导航形态有意分叉**：桌面端是 AppTopBar + 面包屑（DESIGN.md 的刻意简化），Web 应用区
// 是持久侧边栏 / 底部导航——浏览器里没有窗口标题栏可借，且 tab 会被文档站其它页面复用，
// 常驻导航是唯一能表达「这是一个应用而不是一篇文档」的结构。
//
// ## 「有这个页面」与「它进顶级导航」是两件事（2026-08-05）
//
// 五条路由**全部保留**，但常驻导航只列三条（[`APP_NAV`]）：设备 / 收件箱 / 设置。
// 发送与传输是**设备的子页面**（`parent: "devices"`），从设备页进入。
//
// 这不是把 Web 端做薄，是回到另外两端早就在做的事：
//
//   桌面端  顶栏快捷入口只有 收件箱 / 传输 / 设置，**没有发送**；发送从设备卡片进
//   移动端  tab 只有 设备 / 收件箱 / 设置，发送与传输都是从设备页推进去的栈路由
//
// 「发送」尤其不该在这里：DESIGN.md 的 **Send Entry Contract** 写死了「Sending starts from
// a device」，而 `send-panel.tsx` 自己的头注释也说目标选择器「是落点与纠错，不是入口」。
// 一个常驻的「发送」入口把用户领到的正是那条纠错路径——先面对一个空的下拉框，再选一遍
// 他本来在设备页就已经看着的设备。
//
// 代价是清楚的：传输的活跃计数徽标随之离开侧栏。它**必须有新去处**，否则就成了知识库
// 「拆多路由会藏起有时效的东西」那条退化的第二次发生——补偿放在设备页的「活跃传输」区块
// （`active-transfers-section.tsx`），与移动端设备页同一个形态。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import {
  ArrowLeftRight,
  Inbox,
  MonitorSmartphone,
  Send,
  Settings,
  type LucideIcon,
} from "lucide-react";

/**
 * 导航项徽标的数据来源。计数由 app-nav 从 store 派生，本文件保持纯数据。
 *
 * 只有一种，不是遗漏：徽标要回答的是「有件事在等你，而它藏在另一条路由后面」。
 * 待处理 offer 是唯一一件——传输的活跃计数已经由设备页的「活跃传输」区块直接呈现
 * （连着每条会话的进度），不需要在导航上再抽象成一个数字。
 */
export type NavBadgeKind = "offers";

export interface AppNavItem {
  /** 不带尾斜杠；`<Link>` 会按 next.config 的 trailingSlash 自行补全。 */
  href: string;
  /**
   * 页面标题与描述的**可翻译描述符**。渲染时用 `useLingui()` 的 `t(item.label)` 展开。
   *
   * 为什么是描述符而不是字符串：这份定义同时服务两处，而两处的时机不同——
   * 组件在**运行时**渲染（跟随用户 locale），`export const metadata` 在**构建时**求值
   * （静态导出，没有服务端，`<title>` 只能是源 locale）。描述符两边都能用：
   * 组件展开它，metadata 取它的 `message` 字段（即源 locale 原文）。
   *
   * 反过来若存两份（一份纯字符串给 metadata、一份描述符给 UI），改文案时漏掉一份不会有
   * 任何提示——这正是这份「单一事实源」要避免的东西。
   */
  label: MessageDescriptor;
  description: MessageDescriptor;
  icon: LucideIcon;
  badge?: NavBadgeKind;
  /**
   * 归属的顶级导航项。顶级项自己不带这个字段；子页面（发送 / 传输）指向它进得来的那一项。
   *
   * 两个消费者，都靠它避免手写 if-else：侧栏在子页面上仍高亮父项（[`activeNavHref`]），
   * 页头据此渲染一条返回链接（`page-header.tsx`）。没有它，用户在 `/app/send` 会看到
   * 一个三项全灭的侧栏——「我在哪」这个问题突然无人回答。
   */
  parent?: NavKey;
}

/**
 * 导航项的 key。
 *
 * **写成显式联合而不是 `keyof typeof NAV`**：`AppNavItem.parent` 用它，而 `NAV` 又要
 * `satisfies Record<NavKey, AppNavItem>`——从 `NAV` 反推会绕成循环，TS 直接把 `NAV` 推成
 * `any`。显式列出来还多一层好处：这里加一条却忘了在 `NAV` 里给实现，是编译错误。
 */
export type NavKey = "devices" | "send" | "inbox" | "transfer" | "settings";

/**
 * 取描述符的源 locale 原文，供**构建期**求值的 `metadata` 使用。
 *
 * 静态导出下 `<title>` 在构建时就烤进 HTML，没有「当前用户的 locale」可言——所以这里
 * 取源文是正确行为，不是降级。运行时的 UI 一律走 `t(descriptor)`。
 */
export function navTitle(item: AppNavItem): string {
  return item.label.message ?? "";
}

/**
 * 按 key 索引，页面直接 `NAV.devices`——比 `navItem("/app/devices")` 少一次字符串查表，
 * 写错是编译错误而不是运行时才炸。
 *
 * **显式注解而不是 `satisfies`**：后者会把每一项推成各自的字面量类型，于是没写 `parent`
 * 的那几项在联合类型里根本没有这个属性，`item.parent` 直接是编译错误。这里没有任何地方
 * 需要字面量精度（href 不进类型系统，路由是运行时字符串），统一成 `AppNavItem` 更省事。
 */
export const NAV: Record<NavKey, AppNavItem> = {
  devices: {
    href: "/app/devices",
    label: msg`设备`,
    description: msg`已配对设备与配对入口。配对是一次性动作，配完即长期信任。`,
    icon: MonitorSmartphone,
  },
  send: {
    href: "/app/send",
    label: msg`发送`,
    description: msg`选一台已配对设备，拖文件进来直接送达。`,
    icon: Send,
    parent: "devices",
  },
  inbox: {
    href: "/app/inbox",
    label: msg`收件箱`,
    description: msg`已落盘的接收文件；待处理的入站请求也在这里决策。`,
    icon: Inbox,
    badge: "offers",
  },
  transfer: {
    href: "/app/transfer",
    label: msg`传输`,
    description: msg`进行中与已结束的会话。选中一条查看文件明细与续传。`,
    icon: ArrowLeftRight,
    parent: "devices",
  },
  settings: {
    href: "/app/settings",
    label: msg`设置`,
    description: msg`设备信息、引导节点与外观偏好。`,
    icon: Settings,
  },
};

/**
 * 常驻导航的项与顺序：**只有顶级项**（`parent` 为空的那些），与移动端 tab 同序同项。
 *
 * 从 `NAV` 过滤而不是再手写一遍数组：给某一项加 `parent` 就等于把它移出常驻导航，
 * 一处改动、两处生效，不会出现「加了 parent 却还挂在侧栏上」的半迁移状态。
 */
export const APP_NAV: AppNavItem[] = Object.values(NAV).filter((item) => !item.parent);

/** href → 导航项。路由是平的，所以精确查表就够，不做前缀匹配。 */
const BY_HREF = new Map<string, AppNavItem>(Object.values(NAV).map((item) => [item.href, item]));

/**
 * 侧栏 / 底栏该高亮哪一项：子页面回到它的父项。
 *
 * 少了它，`/app/send` 与 `/app/transfer` 下三项全灭——常驻导航的全部意义就是随时回答
 * 「我在哪」，而恰恰在离首页最远的两个页面上它交了白卷。
 *
 * 未登记的路径原样返回（于是不匹配任何一项）。**刻意不前缀匹配**：本区路由是平的，
 * 前缀匹配只会让将来某条 `/app/settings-x` 意外点亮设置。
 */
export function activeNavHref(pathname: string): string {
  const path = normalizePath(pathname);
  const item = BY_HREF.get(path);
  if (!item) return path;
  return item.parent ? NAV[item.parent].href : item.href;
}

/** `/app` 的落点——设备页是应用首页（同桌面端）。 */
export const APP_HOME = NAV.devices.href;

/**
 * query param 名的唯一定义。生产方（拼链接）与消费方（`useSearchParams().get`）都从这里取，
 * 否则同一个契约会在两个文件里各写一份裸字符串。
 */
export const PARAM = {
  /** `/app/send`：预选目标设备。 */
  peerId: "peerId",
  /** `/app/transfer`：选中的会话（静态导出不能用动态路由段，见 transfer-activity-panel.tsx）。 */
  session: "session",
  /** `/app/inbox`：定位到的收件箱条目（同上，条目 id 也是运行时 UUID）。 */
  item: "item",
  /** `/app/inbox`：进入时是否显示已归档条目。见 `inboxItemHref` 说明为什么它必须能进链接。 */
  archived: "archived",
} as const;

export function sendToPeerHref(peerId: string): string {
  return `${NAV.send.href}?${PARAM.peerId}=${encodeURIComponent(peerId)}`;
}

export function transferSessionHref(sessionId: string): string {
  return `${NAV.transfer.href}?${PARAM.session}=${encodeURIComponent(sessionId)}`;
}

/**
 * 定位到某条收件箱条目。
 *
 * `archived` 不是可选装饰：收件箱默认不显示已归档条目，所以一条只带 id 的链接**到不了**
 * 已归档的目标——点进去看到的是一个没有它的列表。生产方通常已经知道目标的归档状态
 * （反查拿到的是完整 detail），把它一并编进链接，比让落地页事后猜要诚实。
 *
 * 这与传输侧「选中项参与历史裁剪」（`groupSessions(sorted, selectedId)`）是同一条纪律：
 * 深链要么保证能到达，要么就别给。
 */
export function inboxItemHref(itemId: string, archived = false): string {
  const base = `${NAV.inbox.href}?${PARAM.item}=${encodeURIComponent(itemId)}`;
  return archived ? `${base}&${PARAM.archived}=1` : base;
}

/** `trailingSlash: true` 下 usePathname() 会带尾斜杠，比较前统一抹平。 */
export function normalizePath(pathname: string): string {
  return pathname.length > 1 && pathname.endsWith("/") ? pathname.slice(0, -1) : pathname;
}
