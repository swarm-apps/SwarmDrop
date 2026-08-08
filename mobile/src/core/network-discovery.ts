import {
  MobileBootstrapCandidateSource,
  MobileDiscoveryMode,
  MobileNatStatus,
  type MobileNetworkRuntimeConfig,
} from "react-native-swarmdrop-core";
import { getMobileBootstrapNodes } from "@/core/bootstrap-nodes";

export type DiscoveryModePreference = "auto" | "lanOnly";

export interface NetworkRuntimePreferences {
  customBootstrapNodes: string[];
  discoveryMode: DiscoveryModePreference;
  autoDiscoverLanHelpers: boolean;
  provideLanHelper: boolean;
  /** 公网可达性：允许经公网中继被跨网设备访问（关闭 = 严格局域网） */
  publicReachability: boolean;
}

export function buildNetworkRuntimeConfig(
  preferences: NetworkRuntimePreferences,
): MobileNetworkRuntimeConfig {
  return {
    bootstrapNodes: getMobileBootstrapNodes(preferences.customBootstrapNodes),
    discoveryMode: discoveryModeToNative(preferences.discoveryMode),
    autoDiscoverLanHelpers: preferences.autoDiscoverLanHelpers,
    provideLanHelper: preferences.provideLanHelper,
    publicReachability: preferences.publicReachability,
  };
}

export function discoveryModeToNative(
  mode: DiscoveryModePreference,
): MobileDiscoveryMode {
  return mode === "lanOnly"
    ? MobileDiscoveryMode.LanOnly
    : MobileDiscoveryMode.Auto;
}

export function discoveryModeFromNative(
  mode?: MobileDiscoveryMode | null,
): DiscoveryModePreference {
  return mode === MobileDiscoveryMode.LanOnly ? "lanOnly" : "auto";
}

export type CandidateSourceKey = "hostConfigured" | "mdnsLanHelper" | "learned";

/**
 * 候选来源 → 稳定 key（既当 React `key`，也当文案分档的判别值）。
 *
 * **三个来源必须各占一个 key。** 此前 `Learned` 走 `default` 分支被折进
 * `hostConfigured`，后果有两层：① 运行时经 identify 学到的候选被显示成「配置节点」，
 * 归因是错的；② 两种来源同时在列表里时 React `key` 直接碰撞。
 */
export function candidateSourceKey(
  source: MobileBootstrapCandidateSource,
): CandidateSourceKey {
  switch (source) {
    case MobileBootstrapCandidateSource.MdnsLanHelper:
      return "mdnsLanHelper";
    case MobileBootstrapCandidateSource.Learned:
      return "learned";
    default:
      return "hostConfigured";
  }
}

/**
 * NAT 是否已被 AutoNAT 确认为公网可达。
 *
 * 原生侧只有 `Public | Unknown` 两态（AutoNAT v2 单次失败不足以判定 Private），
 * 所以这里是布尔而非三态。**别再拿字符串比**——它曾经是 `format!("{:?}")` 出来的
 * `"Public"`，而三处 UI 都写 `=== "public"`，于是这一格恒显示「未知」。
 */
export function isNatMapped(status?: MobileNatStatus | null): boolean {
  return status === MobileNatStatus.Public;
}
