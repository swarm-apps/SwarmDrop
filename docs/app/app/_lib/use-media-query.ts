"use client";

import { useSyncExternalStore } from "react";

/**
 * 全局主从布局断点。**与桌面 `src/hooks/use-media-query.ts` 的 `MASTER_DETAIL_QUERY`
 * 是同一个数，改一处必须改另一处。**
 *
 * 为什么必须是 920 而不是 Tailwind 的 `lg`(1024)：Windows 常见的 125% 缩放下，1200 物理像素
 * 只有 960 CSS 宽——正好落在 920 与 1024 之间。用 1024 会让同一台机器上桌面版分栏、Web 版堆叠。
 *
 * ⚠️ 这条注释此前还写着「设备网格也在这个宽度升到三列」——**已经不是事实**：设备网格改成了
 * `grid-cols-[repeat(auto-fill,minmax(280px,1fr))]`，列数由卡片自己的最小可用宽度决定、不挂断点
 * （理由见 `device-grid.tsx`，以及 `min-[…]` 不能与具名断点并列那条实测）。
 */
export const MASTER_DETAIL_QUERY = "(min-width: 920px)";

/**
 * 设备页「主内容 + 配对侧栏」的分栏断点。**它不是 920，而这是刻意的。**
 *
 * 920 那条量的是**主从**（列表 ↔ 详情，两栏都是内容）；这里量的是「主内容 + 一栏辅助工具」，
 * 两者对宽度的要求不同。更要紧的是 Web 应用区的导航侧栏本身要占位，而它有三档
 * （≥1024 展开 224px · 768–1023 图标 64px · <768 底栏 0），于是**同一个视口宽度在两端剩下的
 * 内容宽并不一样**——桌面端设备页能在 920 就分栏，是因为那儿没有侧栏。
 *
 * 按 1280 这一档算：1280 − 224(侧栏) − 48(`sm:px-6` 两侧) = 1008 内容宽，
 * 减掉 360 的配对栏与 32 的栏间距，主栏 616px —— 正好两列设备卡（280×2 + 8）。
 * 再往下一档（1024 视口）主栏只剩 376px，装不下两列，而单列主栏配一条 360 的侧栏
 * 会让「主」「辅」宽度接近，读起来是两块并列的东西而不是一主一辅。
 *
 * **这个数同时喂 CSS 与 JS**：`devices-section.tsx` 的 `xl:` 栅格与配对面板的默认展开态
 * 必须同时翻转，所以它不能是容器查询（那样 JS 拿不到）。改这里要连 `xl:` 一起改。
 */
export const DEVICES_SPLIT_QUERY = "(min-width: 1280px)";

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

/** ≥1280px：设备页分成「设备网格 + 活跃传输」主栏与「配对」侧栏；否则竖排。 */
export function useIsDevicesSplit(): boolean {
  return useMediaQuery(DEVICES_SPLIT_QUERY);
}
