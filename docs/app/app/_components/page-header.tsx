// 各功能页统一页头。标题与描述取自 `_lib/nav.ts` 的同一条导航项，
// 避免「导航叫设备、页头叫我的设备、tab 叫 Devices」这种三处漂移。

"use client";

import { useLingui } from "@lingui/react/macro";
import { ChevronLeft } from "lucide-react";
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
export function PageHeader({ nav }: { nav: NavKey }) {
  const { t } = useLingui();
  const item = NAV[nav];
  const parent = item.parent ? NAV[item.parent] : null;

  return (
    <header>
      {/*
        子页面（发送 / 传输）的返回入口。**它不是装饰**：这两条路由不在常驻导航里，
        侧栏只把父项标成当前位置——「怎么回去」必须在页面上有一个明确的出口，
        而不是指望用户去按浏览器后退。移动端的 Stack header 天然有这个位置，
        Web 端得自己摆一个。
      */}
      {parent && (
        <Link
          href={parent.href}
          className="focus-ring -ml-1 mb-1 inline-flex min-h-8 items-center gap-0.5 rounded-lg pr-2 pl-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
        >
          <ChevronLeft className="size-3.5" aria-hidden />
          {t(parent.label)}
        </Link>
      )}
      {/*
        20px 而不是此前的 `text-base`（16px）。区块标题（`SectionHeader`）是 15px ——
        页标题只比它大 1px 时，「这一页叫什么」和「这一块叫什么」在视觉上是同一档，
        层级只剩位置在暗示。20 / 15 / 14 / 12 才是四档能分辨的梯子。

        `tracking-[-0.02em]`：字号一上去，默认字距就显得松。收紧的下限是 -0.04em
        （再紧字会粘住），20px 上取一半足够。
      */}
      <h1 className="text-xl font-semibold tracking-[-0.02em] text-foreground">
        {t(item.label)}
      </h1>
      {/* `mt-1`（4px）是组内距：这句描述属于上面那个标题，不是页面的下一块内容。
          它与区块间距（32px）之间那个 8 倍的差就是「谁和谁是一组」的全部说明。 */}
      <p className="mt-1 text-sm text-muted-foreground">{t(item.description)}</p>
    </header>
  );
}
