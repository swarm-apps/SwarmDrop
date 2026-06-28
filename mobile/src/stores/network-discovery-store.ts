import { create } from "zustand";

export type DiscoveryMode = "automatic" | "manual";

interface NetworkDiscoveryState {
  discoveryMode: DiscoveryMode;
  lanHelperEnabled: boolean;
  statusFieldsReady: boolean;
  setDiscoveryMode: (mode: DiscoveryMode) => void;
  setLanHelperEnabled: (enabled: boolean) => void;
}

export const useNetworkDiscoveryStore = create<NetworkDiscoveryState>()(
  (set) => ({
    discoveryMode: "automatic",
    lanHelperEnabled: false,
    statusFieldsReady: false,
    setDiscoveryMode: (discoveryMode) => set({ discoveryMode }),
    setLanHelperEnabled: (lanHelperEnabled) => set({ lanHelperEnabled }),
  }),
);
