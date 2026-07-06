/**
 * usePairingSuccess
 * 监听配对成功状态，自动跳转到设备页面并重置 store
 * 消除多个配对页面中重复的 useEffect 逻辑
 */

import { useEffect } from "react";
import { useNavigate } from "@tanstack/react-router";
import { usePairingStore } from "@/stores/pairing-store";

export function usePairingSuccess() {
  const navigate = useNavigate();
  const phase = usePairingStore((s) => s.current.phase);
  const reset = usePairingStore((s) => s.reset);

  useEffect(() => {
    if (phase === "success") {
      navigate({ to: "/devices" });
      reset();
    }
  }, [phase, navigate, reset]);
}
