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

import {
  ArrowLeftRight,
  Inbox,
  MonitorSmartphone,
  Send,
  Settings,
  type LucideIcon,
} from "lucide-react";

/** 导航项徽标的数据来源。计数由 app-nav 从 store 派生，本文件保持纯数据。 */
export type NavBadgeKind = "offers" | "activeTransfers";

export interface AppNavItem {
  /** 不带尾斜杠；`<Link>` 会按 next.config 的 trailingSlash 自行补全。 */
  href: string;
  label: string;
  description: string;
  icon: LucideIcon;
  badge?: NavBadgeKind;
}

/**
 * 按 key 索引，页面直接 `NAV.devices`——比 `navItem("/app/devices")` 少一次字符串查表，
 * 写错是编译错误而不是运行时才炸。
 */
export const NAV = {
  devices: {
    href: "/app/devices",
    label: "设备",
    description: "已配对设备与配对入口。配对是一次性动作，配完即长期信任。",
    icon: MonitorSmartphone,
  },
  send: {
    href: "/app/send",
    label: "发送",
    description: "选一台已配对设备，拖文件进来直接送达。",
    icon: Send,
  },
  inbox: {
    href: "/app/inbox",
    label: "收件箱",
    description: "已落盘的接收文件；待处理的入站请求也在这里决策。",
    icon: Inbox,
    badge: "offers",
  },
  transfer: {
    href: "/app/transfer",
    label: "传输",
    description: "进行中与已结束的会话。选中一条查看文件明细与续传。",
    icon: ArrowLeftRight,
    badge: "activeTransfers",
  },
  settings: {
    href: "/app/settings",
    label: "设置",
    description: "本机节点身份、helper 连接与开发事件日志。",
    icon: Settings,
  },
} satisfies Record<string, AppNavItem>;

/** 导航渲染顺序（与桌面端 `_app/` 五块同序）。 */
export const APP_NAV: AppNavItem[] = [NAV.devices, NAV.send, NAV.inbox, NAV.transfer, NAV.settings];

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
