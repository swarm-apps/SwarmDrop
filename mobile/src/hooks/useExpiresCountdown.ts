import { useEffect, useState } from "react";

export function useExpiresCountdown(
  expiresAt: Date | number | null | undefined,
  onExpire?: () => void,
): number {
  const [remaining, setRemaining] = useState(() => compute(expiresAt));

  useEffect(() => {
    if (!expiresAt) {
      setRemaining(0);
      return;
    }
    setRemaining(compute(expiresAt));
    let id: ReturnType<typeof setInterval>;
    const tick = () => {
      const next = compute(expiresAt);
      setRemaining(next);
      if (next <= 0) {
        onExpire?.();
        clearInterval(id);
      }
    };
    id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [expiresAt, onExpire]);

  return remaining;
}

function compute(expiresAt: Date | number | null | undefined): number {
  if (!expiresAt) return 0;
  const ms = typeof expiresAt === "number" ? expiresAt : expiresAt.getTime();
  const delta = Math.floor((ms - Date.now()) / 1000);
  return Math.max(0, delta);
}
