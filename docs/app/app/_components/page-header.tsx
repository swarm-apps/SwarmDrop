// 各功能页统一页头。标题与描述取自 `_lib/nav.ts` 的同一条导航项，
// 避免「导航叫设备、页头叫我的设备、tab 叫 Devices」这种三处漂移。

"use client";

import { useLingui } from "@lingui/react/macro";
import { ChevronRight } from "lucide-react";
import Link from "next/link";
import { NAV, type NavKey } from "../_lib/nav";

/**
 * `label` / `description` 是可翻译描述符（见 nav.ts），要在**运行时**展开才跟得上用户的
 * locale——所以本组件是 client component。构建期求值的 `metadata` 走另一条路（`navTitle`）。
 *
 * **入参是导航项的 key 而不是整个对象**：调用方（page.tsx）是 server component，
 * 而 `AppNavItem` 带一个 `icon` 函数组件，函数跨不了 RSC 边界（`next build` 在预渲染时
 * 直接报 "Functions cannot be passed directly to Client Components"）。传一个字符串则
 * 什么都不必序列化，查表在客户端做。
 */
export function PageHeader({
  nav,
  aside,
}: {
  nav: NavKey;
  /**
   * 页头右侧的概览位（设备页的在线 / 已配对 / 传输中三个数）。
   *
   * **它是 `ReactNode` 而不是一个数据 prop**：调用方是 server component，传一个 client
   * component 的 element 只是一个 client reference，不必序列化任何数据；而把「读哪个 store」
   * 塞进本组件会让每个页面的页头都拖上设备页的运行时依赖。
   *
   * 为什么不像桌面端那样另起一整块概览横幅：桌面端那块自带标题与一句定位，而这里
   * `PageHeader` 已经渲染了同样的标题与描述——再加一块就是同一句话说两遍。桌面端
   * `HomeOverview` 的实际形态本来也是「标题在左、统计在右」，补在这里正好补成同一个形态。
   */
  aside?: React.ReactNode;
}) {
  const { t } = useLingui();
  const item = NAV[nav];
  const parent = item.parent ? NAV[item.parent] : null;

  return (
    // `items-end`：统计块与标题下沿对齐，而不是与那行 20px 的标题居中对齐——
    // 页头是「标题 + 描述」两行，垂直居中会让统计浮在两行中间，谁都不贴。
    // 窄屏 `flex-wrap` 让它换行到描述下面，不去挤标题。
    <header className="flex flex-wrap items-end justify-between gap-x-6 gap-y-3">
      {/* 标题组整块是一个 flex 子项：返回链接 / 标题 / 描述三行的竖排关系不受外层 flex 影响。
          `min-w-0` 让它能正常收缩而不是把 `aside` 挤出去。

          **`basis-64` 不能省，也不能写成裸 `flex-1`**：`flex-1` 展开是 `flex: 1 1 0%`，
          basis 为 0 意味着这一项可以一直收缩到零宽，于是 `flex-wrap` 的换行条件**永远不成立**
          ——手机上实测是描述被压成一条 5 行的窄柱、右边杵着三格统计，而不是统计换到下一行。
          给一个 256px 的理想宽度，空间不够时才真的会换行。 */}
      <div className="min-w-0 flex-1 basis-64">
        {/*
          20px 而不是此前的 `text-base`（16px）。区块标题（`SectionHeader`）是 15px ——
          页标题只比它大 1px 时，「这一页叫什么」和「这一块叫什么」在视觉上是同一档，
          层级只剩位置在暗示。20 / 15 / 14 / 12 才是四档能分辨的梯子。

          `tracking-[-0.02em]`：字号一上去，默认字距就显得松。收紧的下限是 -0.04em
          （再紧字会粘住），20px 上取一半足够。

          ## 子页面（发送 / 传输）的标题是**面包屑式**，不是「返回链接 + 标题」两行

          这两条路由不在常驻导航里，侧栏只把父项标成当前位置，所以「怎么回去」必须在页面上
          有明确出口——这一条没变，变的是它的形态。

          此前是标题上方单独一行 `← 设备`。两行结构在这一页里显得比它承载的信息重：页头本就
          只有标题 + 一句描述，再叠一行返回链接，视觉重心全在左上角那个小箭头上。
          写成 `设备 › 传输` 之后出口还在（父项可点），却只占标题自己那一行。

          顺带与**桌面端对齐**：桌面壳的导航就是面包屑（DESIGN.md 的 Navigation — Desktop
          shell，「home icon → 可点的中间段 → 不可点的当前页」）。Web 端分叉的是**常驻导航
          的形态**（侧栏 vs 面包屑），不是「页内怎么表达父子关系」——那件事两端本就该一样。

          ⚠️ 父项链接的命中高度是 **28px**（`py-1` 撑出来的），**不满足 44×44 的触摸标准**。
          如实记在这里而不是假称达标：把它撑到 44 会让标题行高出一截、页头两行变三行的观感，
          而它是次要出口（浏览器后退、侧栏的「设备」都到得了同一个地方）。真要修，入口是
          页头整体的排布，不是给这个链接加 padding。
        */}
        <h1 className="flex flex-wrap items-baseline gap-x-1.5 text-xl font-semibold tracking-[-0.02em] text-foreground">
          {parent && (
            <>
              <Link
                href={parent.href}
                className="focus-ring -mx-1 rounded-lg px-1 py-1 font-normal text-muted-foreground transition-colors hover:text-foreground"
              >
                {t(parent.label)}
              </Link>
              <ChevronRight
                className="size-4 shrink-0 self-center text-muted-foreground/50"
                aria-hidden
              />
            </>
          )}
          <span>{t(item.label)}</span>
        </h1>
        {/* `mt-1`（4px）是组内距：这句描述属于上面那个标题，不是页面的下一块内容。
            它与区块间距（32px）之间那个 8 倍的差就是「谁和谁是一组」的全部说明。 */}
        <p className="mt-1 text-sm text-muted-foreground">{t(item.description)}</p>
      </div>
      {aside}
    </header>
  );
}
