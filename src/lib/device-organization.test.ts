import { describe, expect, it } from "vitest";
import {
  deviceGroupNames,
  deviceIdentityHint,
  emptyDeviceOrganization,
  hasDuplicateOrganizedName,
  organizedDeviceName,
} from "./device-organization";

const device = {
  peerId: "12D3KooWAbCdEfGhIjKlMnOpQrStUvWxYz",
  name: "办公室 Mac",
  hostname: "macbook-pro",
};

describe("device organization display projection", () => {
  it("prefers local alias, then remote name, hostname, and a short PeerId", () => {
    const organization = {
      ...emptyDeviceOrganization,
      aliases: { [device.peerId]: "叶夕月的电脑" },
    };

    expect(organizedDeviceName(device, organization)).toBe("叶夕月的电脑");
    expect(organizedDeviceName({ ...device, peerId: "abc", name: "", hostname: "" }, emptyDeviceOrganization)).toBe("abc");
  });

  it("sorts group labels and exposes identity hints for ambiguous names", () => {
    const organization = {
      aliases: {},
      groups: [
        { id: "work", name: "工作", sortOrder: 1 },
        { id: "family", name: "家人", sortOrder: 0 },
      ],
      groupDeviceIds: { work: [device.peerId], family: [device.peerId] },
    };

    expect(deviceGroupNames(device.peerId, organization)).toEqual(["家人", "工作"]);
    expect(deviceIdentityHint(device)).toContain("macbook-pro · 12D3…UvWxYz");
    expect(hasDuplicateOrganizedName(device, [device, { ...device, peerId: "peer-two" }], organization)).toBe(true);
  });
});
