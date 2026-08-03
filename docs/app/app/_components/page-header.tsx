// 各功能页统一页头。标题与描述取自 `_lib/nav.ts` 的同一条导航项，
// 避免「导航叫设备、页头叫我的设备、tab 叫 Devices」这种三处漂移。

"use client";

import { useLingui } from "@lingui/react/macro";
import { NAV } from "../_lib/nav";

/**
 * `label` / `description` 是可翻译描述符（见 nav.ts），要在**运行时**展开才跟得上用户的
 * locale——所以本组件是 client component。构建期求值的 `metadata` 走另一条路（`navTitle`）。
 *
 * **入参是导航项的 key 而不是整个对象**：调用方（page.tsx）是 server component，
 * 而 `AppNavItem` 带一个 `icon` 函数组件，函数跨不了 RSC 边界（`next build` 在预渲染时
 * 直接报 "Functions cannot be passed directly to Client Components"）。传一个字符串则
 * 什么都不必序列化，查表在客户端做。
 */
export function PageHeader({ nav }: { nav: keyof typeof NAV }) {
  const { t } = useLingui();
  const item = NAV[nav];
  return (
    <header>
      <h1 className="text-base font-semibold text-foreground">{t(item.label)}</h1>
      <p className="mt-1 text-sm text-muted-foreground">{t(item.description)}</p>
    </header>
  );
}
