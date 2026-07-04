/**
 * useMediaQuery
 * 订阅一个 media query，首帧同步取值避免闪烁。
 * 收件箱 / 活动中心的 master-detail 响应式布局共用。
 */

import { useEffect, useState } from "react";

/**
 * 全局主从布局断点：≥920px 双栏，<920px 收成「详情占满 + 列表左抽屉」。
 * 对齐首页设备页的 `min-[920px]` 主分栏——首页收栏时，收件箱/活动中心同步进抽屉。
 * 单一来源：所有 master-detail 页面都用这个，不要各页写各的断点。
 */
export const MASTER_DETAIL_QUERY = "(min-width: 920px)";

/** 主从布局是否处于宽屏（双栏）态。 */
export function useIsWideLayout(): boolean {
  return useMediaQuery(MASTER_DETAIL_QUERY);
}

export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(
    () => typeof window !== "undefined" && window.matchMedia(query).matches,
  );
  useEffect(() => {
    const m = window.matchMedia(query);
    const onChange = () => setMatches(m.matches);
    onChange();
    m.addEventListener("change", onChange);
    return () => m.removeEventListener("change", onChange);
  }, [query]);
  return matches;
}
