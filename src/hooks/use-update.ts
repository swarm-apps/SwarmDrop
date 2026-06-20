import {
  createUpdateEngine,
  type EngineOptions,
  ensureClientId,
  type UpdateEngine,
  type UpdateEngineState,
} from "@swarm-hive/sdk";
import { useUpdateEngine } from "@swarm-hive/sdk/react";
import { getVersion } from "@tauri-apps/api/app";
import { createContext, useContext } from "react";
import { createTauriAdapter, type TauriAdapterOptions } from "@/lib/tauri-adapter";

/** 承载已装配 engine 的 context;由 <UpdateProvider> 注入。 */
export const UpdateEngineContext = createContext<UpdateEngine | null>(null);

/** 订阅当前更新状态。必须在 <UpdateProvider> 内使用。 */
export function useUpdate(): UpdateEngineState {
  const engine = useContext(UpdateEngineContext);
  if (!engine) {
    throw new Error("useUpdate must be used within <UpdateProvider>");
  }
  return useUpdateEngine(engine);
}

export interface CreateSwarmHiveEngineOptions extends TauriAdapterOptions {
  /** 覆盖当前版本;缺省用 @tauri-apps/api/app 的 getVersion()。 */
  currentVersion?: string;
  /** engine 调参(dismissTtlMs / recheckIntervalMs)。 */
  engine?: Partial<Pick<EngineOptions, "dismissTtlMs" | "recheckIntervalMs">>;
}

/**
 * 异步装配 SwarmHive 更新 engine:tauriAdapter + 当前版本(getVersion)+ 持久化的
 * client_id(ensureClientId)。在 <UpdateProvider> 挂载时调用一次。
 */
export async function createSwarmHiveEngine(
  opts: CreateSwarmHiveEngineOptions = {},
): Promise<UpdateEngine> {
  const { currentVersion, engine: engineOpts, ...adapterOpts } = opts;
  const adapter = createTauriAdapter(adapterOpts);
  const [version, clientId] = await Promise.all([
    currentVersion ? Promise.resolve(currentVersion) : getVersion(),
    ensureClientId(adapter.storage),
  ]);
  return createUpdateEngine(adapter, {
    currentVersion: version,
    clientId,
    ...engineOpts,
  });
}
