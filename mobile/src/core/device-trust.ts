import {
  canSendToDevice as canSendToTrustedDevice,
  type PolicyNote,
  policyNoteFor,
  type TrustLevel,
} from "@swarmdrop/shared-view";
import {
  defaultReceivePolicy as coreDefaultReceivePolicy,
  type MobileDevice,
  type MobileDeviceReceivePolicy,
  MobileDeviceTrustLevel,
} from "react-native-swarmdrop-core";

/**
 * 移动端的信任策略模块。
 *
 * **两类东西都不在这里**：
 *
 * - **判定逻辑**（信任级别归一、能否发送、策略归纳）在 `@swarmdrop/shared-view`；
 * - **各级别的默认策略**在 Rust 的 `DeviceReceivePolicy::for_trust_level`，经 uniffi 导出的
 *   `defaultReceivePolicy` 取回。此前这里抄了一份、桌面又抄了一份，两份还长出了不同的
 *   「切级别时保留哪些字段」规则，而内核那一份一个都不保留——同一个产品动作三种行为。
 *
 * 剩下的是一件躲不开平台的事：**uniffi 枚举 ↔ 字符串的映射**。`MobileDeviceTrustLevel`
 * 是运行时枚举对象，而桌面 / Web 拿到的是字符串字面量联合。共享包以字符串为准（那是 wire
 * 上的形态），枚举是 uniffi 的细节，不该外溢——映射留在这里，且在**调用边界**完成转换。
 *
 * **本机落点也不在这里补**（曾经在）。策略留空的含义是「跟随宿主默认」，内核求值时向宿主
 * 取当下那一个（由 `syncReceiveLocationToCore` 推过去）。此前这里会抄一份**当时的**全局
 * 落点进每台设备的策略，用户之后换目录，那台设备的自动接收仍照着旧值写。
 */

export type { PolicyNote, TrustLevel };

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

/** 判定归共享包，本函数只负责把 uniffi 枚举翻成字符串再递过去。 */
export function canSendToDevice(device: MobileDevice): boolean {
  return canSendToTrustedDevice({
    status: device.status,
    trustLevel: resolveTrustLevel(device),
  });
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

/**
 * 某信任级别的默认接收策略。**表在内核**，这里只做枚举转换。
 *
 * `previous` 传该设备当前的策略，用户显式设过的保存位置会被内核带过去（`blocked` 除外）。
 * 切换信任级别时应当传它——不传等于替用户把落点清掉。
 */
export function defaultReceivePolicy(
  level: TrustLevel,
  previous?: MobileDeviceReceivePolicy,
): MobileDeviceReceivePolicy {
  return coreDefaultReceivePolicy(trustLevelToNative(level), previous);
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
    // **不补本机落点**：留空的含义是「跟随宿主默认」，内核在求值时向宿主取当下那一个
    // （`setDefaultSaveLocation`）。此前这里会把**当时的**全局落点抄进来，用户之后换了
    // 目录，这台设备的自动接收仍照着旧值写——或者目录没了之后在接受之后才失败。
    defaultSaveLocation: policy.defaultSaveLocation,
    expiresAt:
      level === "temporary"
        ? (policy.expiresAt ?? defaults.expiresAt)
        : undefined,
  };
}

export { policyNoteFor };
