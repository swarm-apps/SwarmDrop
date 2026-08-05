"use client";

// Web 应用区的常驻导航，三档形态（#90）：
//   ≥1024px  展开侧栏（图标 + 文字）
//   768–1023 图标侧栏（w-16，文字降级为 title/aria-label）
//   <768px   底部导航，配顶部品牌 + 状态条
//
// 三者都是**外壳 flex 布局里的常规子元素**（`shrink-0`），不用 fixed/sticky——
// 应用外壳（layout.tsx）是 `h-dvh` 的受限高度容器，滚动只发生在 `main` 里。
//
// 所有内部跳转必须用 next/link——手写 <a href="/app/devices"> 不会被加上 basePath，
// GitHub Pages 子路径（/SwarmDrop）下会整片 404。
//
// 徽标存在的理由：单页时入站 offer 与页面其它内容同屏可见，拆成多路由后它会藏进 /app/inbox。
// 没有徽标，用户停在发送页时对「有人要发文件给我」零感知——这是重构自己引入的退化，不是新功能。
//
// **只有三项**（设备 / 收件箱 / 设置），与移动端 tab 同项同序。发送与传输是设备的子页面，
// 在这里不占位；侧栏在那两条路由上高亮「设备」（`activeNavHref`）。理由写在 `_lib/nav.ts`。

import { useLingui } from "@lingui/react/macro";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { appIconPath, appName } from "@/lib/shared";
import { APP_NAV, activeNavHref, type AppNavItem } from "../_lib/nav";
import { selectOfferCount, useWebNode } from "../_lib/store";
import { NodeStatusDialog } from "./node-status-dialog";
import { RailTools } from "./rail-tools";

/**
 * 该项的徽标数。计数在**列表外**订阅一次再传进来——hook 不能在 `APP_NAV.map` 里调。
 * selector 返回的是数字，符合「selector 只许返回原始值或稳定引用」（见 store.ts）。
 */
function badgeCount(item: AppNavItem, offers: number): number {
  return item.badge === "offers" ? offers : 0;
}

function useActiveHref(): string {
  return activeNavHref(usePathname());
}

/** 品牌标记。不可点——对齐桌面端「unclickable logo mark」，也避免误点直接退出应用区
 *  （离开 /app 会卸载节点单例，正在进行的传输随之中断）。 */
function BrandMark({ labelClassName = "" }: { labelClassName?: string }) {
  return (
    <span className="inline-flex items-center gap-2 text-sm font-semibold text-foreground">
      <img src={appIconPath} alt="" className="size-5 shrink-0" />
      <span className={labelClassName}>
        {appName}
        <span className="ml-1 font-normal text-muted-foreground">Web</span>
      </span>
    </span>
  );
}

/** 计数徽标：青绿实心底恒配深墨字（DESIGN 的 Brand Fidelity Rule，不用白字）。 */
function CountBadge({ count, className = "" }: { count: number; className?: string }) {
  return (
    <span
      className={`inline-flex min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-semibold tabular-nums text-primary-foreground ${className}`}
    >
      {count > 99 ? "99+" : count}
    </span>
  );
}

// ── 侧栏（≥768px）──────────────────────────────────────────────────────────

export function AppSidebar() {
  const { t } = useLingui();
  const active = useActiveHref();
  const offers = useWebNode(selectOfferCount);

  return (
    // `h-full` 而不是 `sticky top-0 h-screen`：外壳（layout.tsx）现在是 `h-dvh` 的
    // flex 行，侧栏作为它的直接子元素自然满高，不需要 sticky 去模拟。
    //
    // 材质是 `glass-rail`（半透明 + 模糊）而不是实心 `bg-sidebar`：外壳底下现在有一层
    // WebGL 极光，一条 224px 宽的不透明色块会把整个左边缘的光切掉。
    // `relative z-10` 是它压在环境层之上的方式——`.app-shell` 刻意不做通配提升，
    // 理由见 global.css。
    <aside className="glass-rail relative z-10 hidden h-full shrink-0 flex-col border-r md:flex md:w-16 lg:w-56">
      <div className="flex h-14 shrink-0 items-center border-b px-3 md:justify-center lg:justify-start">
        <BrandMark labelClassName="hidden lg:inline" />
      </div>

      <nav aria-label={t`应用导航`} className="min-h-0 flex-1 space-y-1 overflow-y-auto p-2">
        {APP_NAV.map((item) => (
          <SidebarLink
            key={item.href}
            item={item}
            active={active === item.href}
            count={badgeCount(item, offers)}
          />
        ))}
      </nav>

      {/* 底部分两层：**节点状态**（这台机器现在怎么样）与**环境开关**（主题 / 语言 / 文档）。
          前者是状态，后者是调节，混在一起会让那枚状态 pill 读起来也像个设置项——
          而它其实是本页唯一诚实汇报运行时的地方，也是节点启停的唯一入口。 */}
      <div className="shrink-0 space-y-2 border-t p-3 md:flex md:flex-col md:items-center lg:items-stretch">
        <NodeStatusDialog labelClassName="hidden lg:inline" />
        <RailTools />
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
  const { t } = useLingui();
  const Icon = item.icon;
  return (
    <Link
      href={item.href}
      title={t(item.label)}
      aria-current={active ? "page" : undefined}
      className={`focus-ring flex min-h-11 items-center gap-3 rounded-lg px-3 text-sm transition-colors md:justify-center lg:justify-start ${
        active
          ? "bg-accent font-medium text-brand"
          : "text-muted-foreground hover:bg-accent/60 hover:text-foreground"
      }`}
    >
      <span className="relative shrink-0">
        <Icon className="size-[18px]" aria-hidden="true" />
        {/* 图标档没有行尾可放徽标，贴到图标右上角；展开档则走行尾那枚。 */}
        {count > 0 && <CountBadge count={count} className="absolute -top-1.5 -right-2 lg:hidden" />}
      </span>
      <span className="hidden lg:inline">{t(item.label)}</span>
      {count > 0 && <CountBadge count={count} className="ml-auto hidden lg:inline-flex" />}
    </Link>
  );
}

// ── 窄屏顶栏 + 底部导航（<768px）────────────────────────────────────────────

/** 窄屏没有侧栏，品牌与节点状态改由顶栏承担。 */
export function AppMobileHeader() {
  return (
    // 外壳已是受限高度、滚动发生在 main 里，所以顶栏是常规 flex 子元素（`shrink-0`）
    // 而不再需要 `sticky top-0`——它本来就不会随内容滚走了。
    <header className="glass-rail shrink-0 border-b md:hidden">
      <div className="flex items-center justify-between px-4 py-3">
        <BrandMark />
        <NodeStatusDialog />
      </div>
    </header>
  );
}

export function AppBottomNav() {
  const { t } = useLingui();
  const active = useActiveHref();
  const offers = useWebNode(selectOfferCount);

  return (
    // **不再是 `fixed` + 等高 spacer**：外壳（layout.tsx）现在是 `h-dvh` 的 flex 列，
    // 导航作为最后一个 `shrink-0` 子元素天然贴底，滚动只发生在 `main` 里。
    // 于是「知道高度的人和补偿高度的人应当是同一个」这条约定连同那个高度常量一起消失了
    // ——没有补偿，就没有失准的可能。
    <nav
      aria-label={t`应用导航`}
      style={{ paddingBottom: "env(safe-area-inset-bottom)" }}
      className="glass-rail shrink-0 border-t md:hidden"
    >
      <ul className="mx-auto flex max-w-lg">
        {APP_NAV.map((item) => {
          const Icon = item.icon;
          const isActive = active === item.href;
          const count = badgeCount(item, offers);
          return (
            <li key={item.href} className="flex-1">
              <Link
                href={item.href}
                aria-current={isActive ? "page" : undefined}
                className={`focus-ring flex min-h-14 flex-col items-center justify-center gap-1 px-1 text-[11px] transition-colors ${
                  isActive ? "font-medium text-brand" : "text-muted-foreground"
                }`}
              >
                <span className="relative">
                  <Icon className="size-5" aria-hidden="true" />
                  {count > 0 && <CountBadge count={count} className="absolute -top-1.5 -right-2.5" />}
                </span>
                {t(item.label)}
              </Link>
            </li>
          );
        })}
      </ul>
    </nav>
  );
}
