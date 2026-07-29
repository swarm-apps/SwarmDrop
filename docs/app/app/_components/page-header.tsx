// 各功能页统一页头。标题与描述取自 `_lib/nav.ts` 的同一条导航项，
// 避免「导航叫设备、页头叫我的设备、tab 叫 Devices」这种三处漂移。

import type { AppNavItem } from "../_lib/nav";

export function PageHeader({ item }: { item: AppNavItem }) {
  return (
    <header>
      <h1 className="text-base font-semibold text-fd-foreground">{item.label}</h1>
      <p className="mt-1 text-sm text-fd-muted-foreground">{item.description}</p>
    </header>
  );
}
