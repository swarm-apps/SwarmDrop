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
  /** 进入 ready 后自动安装 + 重启,默认 true。 */
  autoInstallOnReady?: boolean;
}

export function UpdateProvider({
  children,
  fallback = null,
  checkOnMount = true,
  recheckOnFocus = true,
  autoInstallOnReady = true,
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

  // 自动安装**只能有一个触发点**。它从前长在 prompt / force / settings 三个组件里,
  // 各自 `useEffect(ready → install)` —— 三者常常同时挂载,于是同一个 ready 会派发三次
  // 安装。此前靠 engine「install 用掉即清句柄」把后两次挡掉了;句柄改为可反复使用后
  // (ready 是持久静止态,移交可能静默失败必须能重试),那层意外的去重就没了。
  // 编排归 Provider —— 它天然是单例。
  useEffect(() => {
    if (!engine || !autoInstallOnReady) return;
    let attemptedVersion: string | null = null;
    const maybeInstall = () => {
      const { status, release } = engine.getState();
      if (status !== "ready" || !release) return;
      // 每个版本只自动装一次:失败后由 UI 上可点的「立即安装」接手,不无限重试。
      if (attemptedVersion === release.version) return;
      attemptedVersion = release.version;
      void engine.getState().install();
    };
    maybeInstall();
    return engine.subscribe(maybeInstall);
  }, [engine, autoInstallOnReady]);

  if (!engine) return <>{fallback}</>;

  return <UpdateEngineContext.Provider value={engine}>{children}</UpdateEngineContext.Provider>;
}
