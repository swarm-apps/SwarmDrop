import { useEffect, useState } from "react";

/**
 * 当前 Unix 秒，按固定间隔自行推进。
 *
 * **tick 的是「现在」而不是「还剩多少」**：剩余量随之变成纯推导，于是同一个组件里的
 * 倒计时、过期判定、列表里每一行的剩余期都读同一个 `now`，不会各自建一个定时器、
 * 也不会出现「码上说还剩 1 分钟、列表里那行还冻在 23 小时」这种自相矛盾。
 *
 * 默认 30 秒：邀请 TTL 是 24 小时，秒级刷新既无意义又白烧渲染；30 秒足够让
 * 「还有 1 分钟」这种末段变化被看见。
 */
export function useNowSeconds(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), intervalMs);
    return () => clearInterval(timer);
  }, [intervalMs]);

  return now;
}
