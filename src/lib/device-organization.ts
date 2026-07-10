import type { Device, PairedDeviceInfo } from "@/lib/bindings";

export interface DeviceGroup {
  id: string;
  name: string;
  sortOrder: number;
}

export interface DeviceOrganization {
  aliases: Record<string, string>;
  groups: DeviceGroup[];
  groupDeviceIds: Record<string, string[]>;
}

export const emptyDeviceOrganization: DeviceOrganization = {
  aliases: {},
  groups: [],
  groupDeviceIds: {},
};

type IdentifiedDevice = Pick<Device | PairedDeviceInfo, "peerId" | "hostname"> & {
  name?: string | null;
};

export function organizedDeviceName(
  device: IdentifiedDevice,
  organization: DeviceOrganization,
): string {
  return organization.aliases[device.peerId]?.trim()
    || device.name?.trim()
    || device.hostname
    || shortPeerId(device.peerId);
}

export function shortPeerId(peerId: string): string {
  return peerId.length > 10
    ? `${peerId.slice(0, 4)}…${peerId.slice(-6)}`
    : peerId;
}

export function deviceIdentityHint(device: IdentifiedDevice): string {
  return `${device.hostname || "未知设备"} · ${shortPeerId(device.peerId)}`;
}

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

export function hasDuplicateOrganizedName(
  device: IdentifiedDevice,
  devices: IdentifiedDevice[],
  organization: DeviceOrganization,
): boolean {
  const name = organizedDeviceName(device, organization);
  return devices.filter((item) => organizedDeviceName(item, organization) === name).length > 1;
}
