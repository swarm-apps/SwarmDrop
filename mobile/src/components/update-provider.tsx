import type { UpdateEngine } from "@swarm-hive/sdk";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { AppState, type AppStateStatus } from "react-native";
import {
  AutoInstallContext,
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
  /** App 回前台(AppState→active)时重新 check(走 engine 节流),默认 true。 */
  recheckOnFocus?: boolean;
  /** 进入 ready 且 app 在前台时自动拉起系统安装器,每个 release 一次,默认 true。 */
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
  // 已自动尝试过安装的 release 版本。ref 供 effect 内同步读,state 供 UI 判断
  // 「自动机会已用掉」(用来把 ready 提示从「点击安装」换成「已取消,可再试」)。
  const attemptedRef = useRef<string | null>(null);
  const [attemptedVersion, setAttemptedVersion] = useState<string | null>(null);
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
    // 回前台兜底:engine.check 内部节流,频繁 active 不会重复打 endpoint;
    // 但对 native 安装路径"返回键关确认框无回调"的悬挂态,这是把状态推回
    // force-required / available 继续劝的关键钩子(check 会按 versionCode 复核)。
    const onChange = (state: AppStateStatus) => {
      if (state === "active") void engine.getState().check();
    };
    const sub = AppState.addEventListener("change", onChange);
    return () => sub.remove();
  }, [engine, recheckOnFocus]);

  // 自动安装**只能有一个触发点**,所以它在 Provider 里(天然单例),不在各组件的 hook 里。
  // 从前它长在 prompt / force / settings 三个组件各自的 effect 中 —— 三者常同时挂载,
  // 于是同一个 ready 派发三次安装、注册三个 AppState 监听。此前靠 engine「install 用掉即
  // 清句柄」意外地去了重;句柄改为可反复使用后(ready 是持久静止态,移交可能静默失败必须
  // 能重试),那层保护就没了。
  useEffect(() => {
    if (!engine || !autoInstallOnReady) return;

    const maybeInstall = () => {
      // **必须在前台**:Android 10+ 会静默丢弃后台派发的 Activity 启动(见 expo-installer)。
      // 与其发出去被吞掉,不如留在 ready 等用户回来 —— 回前台那一刻本 effect 会再跑一次。
      if (AppState.currentState !== "active") return;
      const { status, release, install } = engine.getState();
      if (status !== "ready" || !release) return;
      // 每个 release 只自动试一次:拉起安装框会让 app 短暂离开前台,用户点「取消」再回来
      // 又是一次 active —— 不设记号就会无限弹框。之后的主动权交回给可点的「立即安装」。
      if (attemptedRef.current === release.version) return;
      attemptedRef.current = release.version;
      setAttemptedVersion(release.version);
      void install();
    };

    maybeInstall();
    const unsubscribe = engine.subscribe(maybeInstall);
    const sub = AppState.addEventListener("change", (state: AppStateStatus) => {
      if (state === "active") maybeInstall();
    });
    return () => {
      unsubscribe();
      sub.remove();
    };
  }, [engine, autoInstallOnReady]);

  if (!engine) return <>{fallback}</>;

  return (
    <UpdateEngineContext.Provider value={engine}>
      <AutoInstallContext.Provider value={attemptedVersion}>
        {children}
      </AutoInstallContext.Provider>
    </UpdateEngineContext.Provider>
  );
}
