"use client";

// Web 应用区的常驻导航，三档形态（#90）：
//   ≥1024px  展开侧栏（图标 + 文字）
//   768–1023 图标侧栏（w-16，文字降级为 title/aria-label）
//   <768px   底部导航（fixed + 等高 spacer），配顶部品牌 + 状态条
//
// 所有内部跳转必须用 next/link——手写 <a href="/app/devices"> 不会被加上 basePath，
// GitHub Pages 子路径（/SwarmDrop）下会整片 404。
//
// 徽标存在的理由：单页时入站 offer 与页面其它内容同屏可见，拆成多路由后它会藏进 /app/inbox。
// 没有徽标，用户停在发送页时对「有人要发文件给我」零感知——这是重构自己引入的退化，不是新功能。

import Link from "next/link";
import { usePathname } from "next/navigation";
import { BookText } from "lucide-react";
import { appIconPath, appName } from "@/lib/shared";
import { isActiveSession } from "../_lib/format";
import { APP_NAV, normalizePath, type AppNavItem, type NavBadgeKind } from "../_lib/nav";
import { useWebNode } from "../_lib/store";
import { NodeStatusPill } from "./node-status-pill";

/**
 * 底部导航高度（含 safe-area）。spacer 与 nav 都从这里取，**尺寸只此一处**——
 * layout 不该知道别人的高度，那种魔数会在导航行高一变时悄悄失准。
 */
const BOTTOM_NAV_HEIGHT = "calc(3.75rem + env(safe-area-inset-bottom))";

type BadgeCounts = Record<NavBadgeKind, number>;

/**
 * 徽标计数。两个 selector 都只返回**数字**——在 selector 里 filter/map 出新数组会让
 * `useSyncExternalStore` 每次拿到不等的快照，直接无限重渲染（见 create-store.ts 注释）。
 */
function useBadgeCounts(): BadgeCounts {
  const offers = useWebNode((s) => Object.keys(s.offers).length);
  const activeTransfers = useWebNode((s) =>
    Object.values(s.projections).reduce((n, p) => (isActiveSession(p) ? n + 1 : n), 0),
  );
  return { offers, activeTransfers };
}

function badgeCount(item: AppNavItem, counts: BadgeCounts): number {
  return item.badge ? counts[item.badge] : 0;
}

function useActiveHref(): string {
  return normalizePath(usePathname());
}

/** 品牌标记。不可点——对齐桌面端「unclickable logo mark」，也避免误点直接退出应用区
 *  （离开 /app 会卸载节点单例，正在进行的传输随之中断）。 */
function BrandMark({ labelClassName = "" }: { labelClassName?: string }) {
  return (
    <span className="inline-flex items-center gap-2 text-sm font-semibold text-fd-foreground">
      <img src={appIconPath} alt="" className="size-5 shrink-0" />
      <span className={labelClassName}>
        {appName}
        <span className="ml-1 font-normal text-fd-muted-foreground">Web</span>
      </span>
    </span>
  );
}

/** 计数徽标：青绿实心底恒配深墨字（DESIGN 的 Brand Fidelity Rule，不用白字）。 */
function CountBadge({ count, className = "" }: { count: number; className?: string }) {
  return (
    <span
      className={`inline-flex min-w-4 items-center justify-center rounded-full bg-[var(--brand-solid)] px-1 text-[10px] font-semibold tabular-nums text-[var(--brand-ink)] ${className}`}
    >
      {count > 99 ? "99+" : count}
    </span>
  );
}

// ── 侧栏（≥768px）──────────────────────────────────────────────────────────

export function AppSidebar() {
  const active = useActiveHref();
  const counts = useBadgeCounts();

  return (
    <aside className="sticky top-0 hidden h-screen shrink-0 flex-col border-r border-fd-border bg-fd-card/40 md:flex md:w-16 lg:w-56">
      <div className="flex h-14 shrink-0 items-center border-b border-fd-border px-3 md:justify-center lg:justify-start">
        <BrandMark labelClassName="hidden lg:inline" />
      </div>

      <nav aria-label="应用导航" className="flex-1 space-y-1 overflow-y-auto p-2">
        {APP_NAV.map((item) => (
          <SidebarLink
            key={item.href}
            item={item}
            active={active === item.href}
            count={badgeCount(item, counts)}
          />
        ))}
      </nav>

      <div className="shrink-0 space-y-2 border-t border-fd-border p-3 md:flex md:flex-col md:items-center lg:items-stretch">
        <NodeStatusPill labelClassName="hidden lg:inline" />
        <Link
          href="/docs"
          title="使用文档"
          className="inline-flex items-center gap-2 rounded-lg px-2 py-1.5 text-xs text-fd-muted-foreground transition-colors hover:bg-fd-accent hover:text-fd-foreground md:justify-center lg:justify-start"
        >
          <BookText className="size-4 shrink-0" aria-hidden="true" />
          <span className="hidden lg:inline">使用文档</span>
        </Link>
      </div>
    </aside>
  );
}

function SidebarLink({
  item,
  active,
  count,
}: {
  item: AppNavItem;
  active: boolean;
  count: number;
}) {
  const Icon = item.icon;
  return (
    <Link
      href={item.href}
      title={item.label}
      aria-current={active ? "page" : undefined}
      className={`flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors md:justify-center lg:justify-start ${
        active
          ? "bg-fd-accent font-medium text-[var(--brand)]"
          : "text-fd-muted-foreground hover:bg-fd-accent/60 hover:text-fd-foreground"
      }`}
    >
      <span className="relative shrink-0">
        <Icon className="size-[18px]" aria-hidden="true" />
        {/* 图标档没有行尾可放徽标，贴到图标右上角；展开档则走行尾那枚。 */}
        {count > 0 && <CountBadge count={count} className="absolute -top-1.5 -right-2 lg:hidden" />}
      </span>
      <span className="hidden lg:inline">{item.label}</span>
      {count > 0 && <CountBadge count={count} className="ml-auto hidden lg:inline-flex" />}
    </Link>
  );
}

// ── 窄屏顶栏 + 底部导航（<768px）────────────────────────────────────────────

/** 窄屏没有侧栏，品牌与节点状态改由顶栏承担。 */
export function AppMobileHeader() {
  return (
    <header className="sticky top-0 z-10 border-b border-fd-border bg-fd-background/95 backdrop-blur md:hidden">
      <div className="flex items-center justify-between px-4 py-3">
        <BrandMark />
        <NodeStatusPill />
      </div>
    </header>
  );
}

export function AppBottomNav() {
  const active = useActiveHref();
  const counts = useBadgeCounts();

  return (
    <>
      {/* nav 是 fixed 的，这块等高占位让内容不被压在导航下面。放在这里而不是让 layout
          写死一个 padding：知道高度的人和补偿高度的人应当是同一个。 */}
      <div aria-hidden="true" className="md:hidden" style={{ height: BOTTOM_NAV_HEIGHT }} />
      <nav
        aria-label="应用导航"
        style={{ paddingBottom: "env(safe-area-inset-bottom)" }}
        className="fixed inset-x-0 bottom-0 z-20 border-t border-fd-border bg-fd-background/95 backdrop-blur md:hidden"
      >
        <ul className="mx-auto flex max-w-lg">
          {APP_NAV.map((item) => {
            const Icon = item.icon;
            const isActive = active === item.href;
            const count = badgeCount(item, counts);
            return (
              <li key={item.href} className="flex-1">
                <Link
                  href={item.href}
                  aria-current={isActive ? "page" : undefined}
                  className={`flex flex-col items-center gap-1 px-1 py-2 text-[11px] transition-colors ${
                    isActive ? "font-medium text-[var(--brand)]" : "text-fd-muted-foreground"
                  }`}
                >
                  <span className="relative">
                    <Icon className="size-5" aria-hidden="true" />
                    {count > 0 && <CountBadge count={count} className="absolute -top-1.5 -right-2.5" />}
                  </span>
                  {item.label}
                </Link>
              </li>
            );
          })}
        </ul>
      </nav>
    </>
  );
}
