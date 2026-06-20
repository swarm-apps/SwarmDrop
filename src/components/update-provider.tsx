import type { UpdateEngine } from "@swarm-hive/sdk";
import { type ReactNode, useEffect, useRef, useState } from "react";
import {
  type CreateSwarmHiveEngineOptions,
  createSwarmHiveEngine,
  UpdateEngineContext,
} from "@/hooks/use-update";

export interface UpdateProviderProps extends CreateSwarmHiveEngineOptions {
  children: ReactNode;
  /** engine 装配完成前(取版本 + client_id,通常不到一帧)显示的内容,默认 null。 */
  fallback?: ReactNode;
  /** 挂载后自动 check 一次,默认 true。 */
  checkOnMount?: boolean;
  /** 窗口重新获得焦点时重新 check(走 engine 节流),默认 true。 */
  recheckOnFocus?: boolean;
}

export function UpdateProvider({
  children,
  fallback = null,
  checkOnMount = true,
  recheckOnFocus = true,
  ...engineOpts
}: UpdateProviderProps) {
  const [engine, setEngine] = useState<UpdateEngine | null>(null);
  // engineOpts 仅首次装配用;后续变化不重建 engine(避免丢失下载状态)。
  const optsRef = useRef(engineOpts);

  useEffect(() => {
    let cancelled = false;
    void createSwarmHiveEngine(optsRef.current).then((created) => {
      if (cancelled) return;
      setEngine(created);
      if (checkOnMount) void created.getState().check();
    });
    return () => {
      cancelled = true;
    };
  }, [checkOnMount]);

  useEffect(() => {
    if (!engine || !recheckOnFocus) return;
    const onFocus = () => void engine.getState().check();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [engine, recheckOnFocus]);

  if (!engine) return <>{fallback}</>;

  return <UpdateEngineContext.Provider value={engine}>{children}</UpdateEngineContext.Provider>;
}
