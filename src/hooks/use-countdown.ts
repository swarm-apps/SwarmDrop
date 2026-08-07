import { useState, useEffect } from "react";

/**
 * 倒计时 Hook
 * @param expiresAt 到期时刻（毫秒时间戳或 ISO 8601 字符串），为 null 时不启动倒计时
 * @returns { remainingSeconds, isExpired }
 *
 * `isExpired` 必须是 state，不能在渲染期用 `Date.now() >= expiresMs` 现算：
 * 剩余秒数归零后 `setRemainingSeconds(0)` 传入相同值会被 React bailout，组件不再重渲染，
 * 现算的 isExpired 就永远停在过期前那次快照上——配对页卡在「将在 0:00 后过期」，
 * 过期态永远不出现（2026-07 实机验证抓到）。两个原始值 state 各自享受 bailout，
 * 不需要手写相等比较；到期即停表。
 */
export function useCountdown(expiresAt: number | string | null) {
  const expiresMs =
    typeof expiresAt === "string" ? new Date(expiresAt).getTime() : expiresAt;
  const [remainingSeconds, setRemainingSeconds] = useState(0);
  const [isExpired, setIsExpired] = useState(false);

  useEffect(() => {
    if (expiresMs == null) {
      setRemainingSeconds(0);
      setIsExpired(false);
      return;
    }

    let interval: ReturnType<typeof setInterval> | undefined;
    const update = () => {
      const remainingMs = expiresMs - Date.now();
      setRemainingSeconds(Math.max(0, Math.floor(remainingMs / 1000)));
      setIsExpired(remainingMs <= 0);
      if (remainingMs <= 0) clearInterval(interval);
    };

    update();
    interval = setInterval(update, 1000);
    // 首帧就已过期时上面那次 update 还拿不到 interval，这里补一次停表
    if (expiresMs - Date.now() <= 0) clearInterval(interval);
    return () => clearInterval(interval);
  }, [expiresMs]);

  return { remainingSeconds, isExpired };
}
