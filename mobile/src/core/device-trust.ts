import type { MobileDevice } from "react-native-swarmdrop-core";

export type TrustLevel = "owned" | "collaborator" | "temporary" | "blocked";

type DeviceWithTrust = MobileDevice & {
  trustLevel?: TrustLevel | null;
};

export function resolveTrustLevel(device: MobileDevice): TrustLevel {
  return (device as DeviceWithTrust).trustLevel ?? "collaborator";
}

export function canSendToDevice(device: MobileDevice): boolean {
  return device.status === "online" && resolveTrustLevel(device) !== "blocked";
}

export function policySummaryForDevice(device: MobileDevice): {
  level: TrustLevel;
  receivePolicyReady: boolean;
  note: "placeholder" | "blocked";
} {
  const level = resolveTrustLevel(device);
  return {
    level,
    receivePolicyReady: false,
    note: level === "blocked" ? "blocked" : "placeholder",
  };
}
