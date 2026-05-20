import { useState, useEffect } from "react";

/**
 * 倒计时 Hook
 * @param expiresAt ISO 8601 字符串（后端 chrono::DateTime<Utc>），为 null 时不启动倒计时
 * @returns { remainingSeconds, isExpired }
 */
export function useCountdown(expiresAt: string | null) {
  const expiresMs = expiresAt != null ? new Date(expiresAt).getTime() : null;
  const [remainingSeconds, setRemainingSeconds] = useState(0);

  useEffect(() => {
    if (expiresMs == null) return;

    const update = () => {
      setRemainingSeconds(Math.max(0, Math.floor((expiresMs - Date.now()) / 1000)));
    };

    update();
    const interval = setInterval(update, 1000);
    return () => clearInterval(interval);
  }, [expiresMs]);

  // 直接从 expiresMs 判断，避免 remainingSeconds 初始为 0 时的误判
  const isExpired = expiresMs != null && Date.now() >= expiresMs;

  return { remainingSeconds, isExpired };
}
