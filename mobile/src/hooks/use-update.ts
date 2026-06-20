import {
  createUpdateEngine,
  type EngineOptions,
  ensureClientId,
  type UpdateEngine,
  type UpdateEngineState,
} from "@swarm-hive/sdk";
import { useUpdateEngine } from "@swarm-hive/sdk/react";
import * as Application from "expo-application";
import * as Crypto from "expo-crypto";
import { createContext, useContext } from "react";
import { createExpoApkDownloader } from "@/lib/expo-downloader";
import { createExpoApkInstaller } from "@/lib/expo-installer";
import { createRnAdapter, type RnAdapterOptions } from "@/lib/rn-adapter";
import { createAsyncStorage } from "@/lib/rn-storage";

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

export interface CreateSwarmHiveEngineOptions
  extends Pick<RnAdapterOptions, "baseUrl" | "appSlug">,
    Partial<
      Pick<
        RnAdapterOptions,
        | "currentVersionName"
        | "abi"
        | "channel"
        | "downloader"
        | "installer"
        | "storage"
        | "fetchImpl"
      >
    > {
  /**
   * 覆盖当前 versionCode(字符串)。缺省用 expo-application 的 nativeBuildVersion——
   * Android 上即 versionCode;RN 整数闸门以它为主键(见 SDK versionCodeComparator)。
   */
  currentVersion?: string;
  /** engine 调参(dismissTtlMs / recheckIntervalMs)。 */
  engine?: Partial<Pick<EngineOptions, "dismissTtlMs" | "recheckIntervalMs">>;
}

/**
 * 异步装配 SwarmHive 更新 engine:rnAdapter + 当前 versionCode(nativeBuildVersion)+
 * 持久化的 client_id(ensureClientId,RN 强制传 expo-crypto 的 randomUUID)。
 * 在 <UpdateProvider> 挂载时调用一次。
 *
 * 只必填 `baseUrl` + `appSlug`;downloader/installer/storage 缺省用 expo 实现
 * (createExpoApkDownloader / createExpoApkInstaller / createAsyncStorage),开箱即用;
 * currentVersionName 缺省取 nativeApplicationVersion。需要换实现时再注入覆盖。
 */
export async function createSwarmHiveEngine(
  opts: CreateSwarmHiveEngineOptions,
): Promise<UpdateEngine> {
  const { currentVersion, engine: engineOpts, ...rest } = opts;
  const adapter = createRnAdapter({
    baseUrl: rest.baseUrl,
    appSlug: rest.appSlug,
    // Android: nativeApplicationVersion = versionName(显示用);缺省兜底 "0"。
    currentVersionName:
      rest.currentVersionName ?? Application.nativeApplicationVersion ?? "0",
    abi: rest.abi,
    channel: rest.channel,
    downloader: rest.downloader ?? createExpoApkDownloader(),
    installer: rest.installer ?? createExpoApkInstaller(),
    storage: rest.storage ?? createAsyncStorage(),
    fetchImpl: rest.fetchImpl,
  });
  // Android: nativeBuildVersion = versionCode;缺省兜底 "0"——让首次检查必判"有更新"而非崩。
  const version = currentVersion ?? Application.nativeBuildVersion ?? "0";
  const clientId = await ensureClientId(adapter.storage, () =>
    Crypto.randomUUID(),
  );
  return createUpdateEngine(adapter, {
    currentVersion: version,
    clientId,
    ...engineOpts,
  });
}
