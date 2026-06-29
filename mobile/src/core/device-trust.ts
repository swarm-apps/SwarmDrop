import {
  type MobileDevice,
  type MobileDeviceReceivePolicy,
  MobileDeviceTrustLevel,
  MobileReceiveSaveBehavior,
} from "react-native-swarmdrop-core";
import { resolveReceiveLocation } from "@/core/paths";

export type TrustLevel = "owned" | "collaborator" | "temporary" | "blocked";

export type PolicyNote =
  | "auto_accept"
  | "manual_confirmation"
  | "temporary"
  | "blocked";

export function resolveTrustLevel(device: MobileDevice): TrustLevel {
  return trustLevelFromNative(device.trustLevel);
}

export function trustLevelFromNative(
  level?: MobileDeviceTrustLevel | null,
): TrustLevel {
  switch (level) {
    case MobileDeviceTrustLevel.Owned:
      return "owned";
    case MobileDeviceTrustLevel.Temporary:
      return "temporary";
    case MobileDeviceTrustLevel.Blocked:
      return "blocked";
    default:
      return "collaborator";
  }
}

export function trustLevelToNative(level: TrustLevel): MobileDeviceTrustLevel {
  switch (level) {
    case "owned":
      return MobileDeviceTrustLevel.Owned;
    case "temporary":
      return MobileDeviceTrustLevel.Temporary;
    case "blocked":
      return MobileDeviceTrustLevel.Blocked;
    default:
      return MobileDeviceTrustLevel.Collaborator;
  }
}

export function canSendToDevice(device: MobileDevice): boolean {
  return device.status === "online" && resolveTrustLevel(device) !== "blocked";
}

export function policyForDevice(
  device: MobileDevice,
): MobileDeviceReceivePolicy {
  return (
    device.receivePolicy ?? defaultReceivePolicy(resolveTrustLevel(device))
  );
}

export function policySummaryForDevice(device: MobileDevice): {
  level: TrustLevel;
  policy: MobileDeviceReceivePolicy;
  receivePolicyReady: boolean;
  note: PolicyNote;
} {
  const level = resolveTrustLevel(device);
  const policy = policyForDevice(device);
  return {
    level,
    policy,
    receivePolicyReady: device.receivePolicy != null,
    note: policyNoteFor(level, policy),
  };
}

export function defaultReceivePolicy(
  level: TrustLevel,
): MobileDeviceReceivePolicy {
  switch (level) {
    case "owned":
      return {
        autoAccept: true,
        requireConfirmation: false,
        maxTransferBytes: undefined,
        allowDirectories: true,
        allowRelayAutoAccept: true,
        saveBehavior: MobileReceiveSaveBehavior.InboxAndDefaultSaveLocation,
        defaultSaveLocation: resolveReceiveLocation(),
        allowMcpSendToDevice: true,
        expiresAt: undefined,
      };
    case "temporary":
      return {
        autoAccept: false,
        requireConfirmation: true,
        maxTransferBytes: 512n * 1024n * 1024n,
        allowDirectories: false,
        allowRelayAutoAccept: false,
        saveBehavior: MobileReceiveSaveBehavior.InboxAndDefaultSaveLocation,
        defaultSaveLocation: undefined,
        allowMcpSendToDevice: false,
        expiresAt: BigInt(Date.now() + 24 * 60 * 60 * 1000),
      };
    case "blocked":
      return {
        autoAccept: false,
        requireConfirmation: false,
        maxTransferBytes: 0n,
        allowDirectories: false,
        allowRelayAutoAccept: false,
        saveBehavior: MobileReceiveSaveBehavior.InboxAndDefaultSaveLocation,
        defaultSaveLocation: undefined,
        allowMcpSendToDevice: false,
        expiresAt: undefined,
      };
    default:
      return {
        autoAccept: false,
        requireConfirmation: true,
        maxTransferBytes: undefined,
        allowDirectories: true,
        allowRelayAutoAccept: false,
        saveBehavior: MobileReceiveSaveBehavior.InboxAndDefaultSaveLocation,
        defaultSaveLocation: undefined,
        allowMcpSendToDevice: false,
        expiresAt: undefined,
      };
  }
}

export function policyWithTrustDefaults(
  level: TrustLevel,
  existing?: MobileDeviceReceivePolicy,
): MobileDeviceReceivePolicy {
  if (!existing) return defaultReceivePolicy(level);
  const defaults = defaultReceivePolicy(level);
  if (level === "blocked") return defaults;
  if (level === "owned") {
    return {
      ...defaults,
      maxTransferBytes: existing.maxTransferBytes,
      allowDirectories: existing.allowDirectories,
      defaultSaveLocation:
        existing.defaultSaveLocation ?? resolveReceiveLocation(),
      expiresAt: undefined,
    };
  }
  if (level === "temporary") {
    return {
      ...defaults,
      expiresAt: existing.expiresAt ?? defaults.expiresAt,
    };
  }
  return {
    ...defaults,
    maxTransferBytes: existing.maxTransferBytes,
    allowDirectories: existing.allowDirectories,
  };
}

export function normalizePolicyForTrustLevel(
  level: TrustLevel,
  policy: MobileDeviceReceivePolicy,
): MobileDeviceReceivePolicy {
  if (level === "blocked") return defaultReceivePolicy("blocked");

  const defaults = defaultReceivePolicy(level);
  const autoAccept = policy.autoAccept && !policy.requireConfirmation;
  return {
    ...policy,
    autoAccept,
    requireConfirmation: autoAccept ? false : policy.requireConfirmation,
    allowRelayAutoAccept: autoAccept ? policy.allowRelayAutoAccept : false,
    saveBehavior: policy.saveBehavior ?? defaults.saveBehavior,
    defaultSaveLocation:
      autoAccept && !policy.defaultSaveLocation
        ? defaults.defaultSaveLocation
        : policy.defaultSaveLocation,
    expiresAt:
      level === "temporary"
        ? (policy.expiresAt ?? defaults.expiresAt)
        : undefined,
  };
}

export function policyNoteFor(
  level: TrustLevel,
  policy: MobileDeviceReceivePolicy,
): PolicyNote {
  if (level === "blocked") return "blocked";
  if (level === "temporary") return "temporary";
  if (policy.autoAccept && !policy.requireConfirmation) return "auto_accept";
  return "manual_confirmation";
}
