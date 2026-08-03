"use client";

import { useSyncExternalStore } from "react";

/**
 * 全局主从布局断点。**与桌面 `src/hooks/use-media-query.ts` 的 `MASTER_DETAIL_QUERY`
 * 是同一个数，改一处必须改另一处。**
 *
 * 为什么必须是 920 而不是 Tailwind 的 `lg`(1024)：Windows 常见的 125% 缩放下，1200 物理像素
 * 只有 960 CSS 宽——正好落在 920 与 1024 之间。用 1024 会让同一台机器上桌面版分栏、Web 版堆叠。
 *
 * 设备网格也在这个宽度升到三列（见 `device-grid.tsx`），于是整个应用区在同一个断点一起换形态，
 * 而不是各页各挑一个数。
 */
export const MASTER_DETAIL_QUERY = "(min-width: 920px)";

/**
 * 订阅一条媒体查询。
 *
 * 用 `useSyncExternalStore` 而不是 `useState` + effect：**服务端快照显式返回窄屏**，于是
 * 静态导出的预渲染 HTML 与客户端首帧一致，不会因为「构建期猜宽屏、客户端发现是窄屏」而
 * hydration mismatch。移动优先在这里也是默认值的方向。
 */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const list = window.matchMedia(query);
      list.addEventListener("change", onChange);
      return () => list.removeEventListener("change", onChange);
    },
    () => window.matchMedia(query).matches,
    () => false,
  );
}

/** ≥920px 为宽屏：左列表 + 右详情双栏；否则详情占满、列表进抽屉。 */
export function useIsWideLayout(): boolean {
  return useMediaQuery(MASTER_DETAIL_QUERY);
}
