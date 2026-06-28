import {
  MobileBootstrapCandidateSource,
  MobileDiscoveryMode,
  type MobileNetworkRuntimeConfig,
} from "react-native-swarmdrop-core";

export type DiscoveryModePreference = "auto" | "lanOnly";

export interface NetworkRuntimePreferences {
  customBootstrapNodes: string[];
  discoveryMode: DiscoveryModePreference;
  autoDiscoverLanHelpers: boolean;
  provideLanHelper: boolean;
}

export function buildNetworkRuntimeConfig(
  preferences: NetworkRuntimePreferences,
): MobileNetworkRuntimeConfig {
  return {
    customBootstrapNodes: preferences.customBootstrapNodes,
    discoveryMode: discoveryModeToNative(preferences.discoveryMode),
    autoDiscoverLanHelpers: preferences.autoDiscoverLanHelpers,
    provideLanHelper: preferences.provideLanHelper,
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

export function candidateSourceKey(
  source: MobileBootstrapCandidateSource,
): "builtInPublic" | "userCustom" | "mdnsLanHelper" {
  switch (source) {
    case MobileBootstrapCandidateSource.UserCustom:
      return "userCustom";
    case MobileBootstrapCandidateSource.MdnsLanHelper:
      return "mdnsLanHelper";
    default:
      return "builtInPublic";
  }
}
