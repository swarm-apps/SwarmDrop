/**
 * 本机设备组织 —— 别名 + 分组的显示投影。
 *
 * 与桌面端 `src/lib/device-organization.ts` 同构：别名 / 分组只保存在本机
 * （preferences-store），不写入 keychain 的 `PairedDeviceInfo`，也不同步给对端。
 * 本文件保持 i18n-free（纯投影逻辑），文案由调用方负责本地化。
 */

export interface DeviceGroup {
  id: string;
  name: string;
  sortOrder: number;
}

export interface DeviceOrganization {
  /** PeerId → 本机别名（已 trim，非空） */
  aliases: Record<string, string>;
  groups: DeviceGroup[];
  /** 分组 id → 该组内的 PeerId 列表 */
  groupDeviceIds: Record<string, string[]>;
}

export const emptyDeviceOrganization: DeviceOrganization = {
  aliases: {},
  groups: [],
  groupDeviceIds: {},
};

/** 分组按 sortOrder 排序 —— 全 app 统一的分组排序入口。 */
export function sortedDeviceGroups(
  organization: DeviceOrganization,
): DeviceGroup[] {
  return [...organization.groups].sort((a, b) => a.sortOrder - b.sortOrder);
}

/** 任何含 `{ peerId, hostname, name? }` 形状的设备（MobileDevice / PairedDeviceSummary）。 */
export interface IdentifiedDevice {
  peerId: string;
  hostname: string;
  name?: string | null;
}

/** 显示名优先级：本机别名 → 对端 name → hostname → 短 PeerId。 */
export function organizedDeviceName(
  device: IdentifiedDevice,
  organization: DeviceOrganization,
): string {
  return (
    organization.aliases[device.peerId]?.trim() ||
    device.name?.trim() ||
    device.hostname ||
    shortPeerId(device.peerId)
  );
}

export function shortPeerId(peerId: string): string {
  return peerId.length > 10
    ? `${peerId.slice(0, 4)}…${peerId.slice(-6)}`
    : peerId;
}

/** 次级身份提示：`hostname · 短 PeerId`，同名消歧用。 */
export function deviceIdentityHint(device: IdentifiedDevice): string {
  return `${device.hostname || shortPeerId(device.peerId)} · ${shortPeerId(device.peerId)}`;
}

/** 该设备按 sortOrder 排序后的所属分组名。 */
export function deviceGroupNames(
  peerId: string,
  organization: DeviceOrganization,
): string[] {
  const groupIds = new Set(
    Object.entries(organization.groupDeviceIds)
      .filter(([, deviceIds]) => deviceIds.includes(peerId))
      .map(([groupId]) => groupId),
  );
  return [...organization.groups]
    .filter((group) => groupIds.has(group.id))
    .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
    .map((group) => group.name);
}

/** 该设备的显示名是否在给定列表中重复（用于决定是否展示次级身份信息）。 */
export function hasDuplicateOrganizedName(
  device: IdentifiedDevice,
  devices: IdentifiedDevice[],
  organization: DeviceOrganization,
): boolean {
  const name = organizedDeviceName(device, organization);
  return (
    devices.filter((item) => organizedDeviceName(item, organization) === name)
      .length > 1
  );
}

/**
 * 归一化任意持久化值为合法 `DeviceOrganization` —— 旧偏好 / 缺字段 / 格式过旧时
 * 退化为空组织；丢弃非法分组、剔除悬空成员关系与空别名。
 */
export function normalizeDeviceOrganization(
  value: unknown,
): DeviceOrganization {
  if (!value || typeof value !== "object") {
    return { aliases: {}, groups: [], groupDeviceIds: {} };
  }

  const source = value as Record<string, unknown>;
  const groups = Array.isArray(source.groups)
    ? source.groups.flatMap((group, sortOrder) => {
        if (!group || typeof group !== "object") return [];
        const candidate = group as Record<string, unknown>;
        if (
          typeof candidate.id !== "string" ||
          typeof candidate.name !== "string"
        ) {
          return [];
        }
        return [
          {
            id: candidate.id,
            name: candidate.name,
            sortOrder:
              typeof candidate.sortOrder === "number"
                ? candidate.sortOrder
                : sortOrder,
          },
        ];
      })
    : [];
  const groupIds = new Set(groups.map((group) => group.id));
  const aliases = Object.fromEntries(
    Object.entries((source.aliases as Record<string, unknown>) ?? {}).filter(
      ([, alias]) => typeof alias === "string" && alias.trim(),
    ),
  ) as Record<string, string>;
  const groupDeviceIds = Object.fromEntries(
    Object.entries((source.groupDeviceIds as Record<string, unknown>) ?? {})
      .filter(([groupId]) => groupIds.has(groupId))
      .map(([groupId, peerIds]) => [
        groupId,
        Array.isArray(peerIds)
          ? peerIds.filter(
              (peerId): peerId is string => typeof peerId === "string",
            )
          : [],
      ]),
  ) as Record<string, string[]>;

  return { aliases, groups, groupDeviceIds };
}
