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
import { resolveReceiveLocation } from "@/core/paths";

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
 * 剩下的是两件躲不开平台的事：
 *
 * 1. **uniffi 枚举 ↔ 字符串的映射**。`MobileDeviceTrustLevel` 是运行时枚举对象，而桌面 /
 *    Web 拿到的是字符串字面量联合。共享包以字符串为准（那是 wire 上的形态），枚举是 uniffi
 *    的细节，不该外溢——映射留在这里，且在**调用边界**完成转换。
 * 2. **本机的默认落点**。内核不知道「这台手机把文件放哪」，那是宿主的知识
 *    （用户偏好 `receivePath`，回退应用文档目录）。见 [`withHostSaveLocation`]。
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
 * 补上**本机的**默认落点。
 *
 * 内核给不出它——`default_save_location` 是一个 host 路径，只有宿主知道自己把文件放哪。
 * 只在会自动接收时补：其余级别本来就要逐次确认，落点由那次确认给。`blocked` 的
 * `autoAccept` 恒为 false，因此天然不会被补回一个落点。
 *
 * **不补会怎样**：`evaluate_receive_policy` 在这一项为空时一律退回手动确认
 * （「未配置自动接收保存位置」）——也就是自动接收的开关开着但不生效。
 */
function withHostSaveLocation(
  policy: MobileDeviceReceivePolicy,
): MobileDeviceReceivePolicy {
  if (!policy.autoAccept || policy.defaultSaveLocation) return policy;
  return { ...policy, defaultSaveLocation: resolveReceiveLocation() };
}

/**
 * 某信任级别的默认接收策略。**表在内核**，这里只做两件事：枚举转换 + 补本机落点。
 *
 * `previous` 传该设备当前的策略，用户显式设过的保存位置会被内核带过去（`blocked` 除外）。
 * 切换信任级别时应当传它——不传等于替用户把落点清掉。
 */
export function defaultReceivePolicy(
  level: TrustLevel,
  previous?: MobileDeviceReceivePolicy,
): MobileDeviceReceivePolicy {
  return withHostSaveLocation(
    coreDefaultReceivePolicy(trustLevelToNative(level), previous),
  );
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

export { policyNoteFor };
