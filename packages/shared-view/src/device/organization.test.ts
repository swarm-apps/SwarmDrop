import { describe, expect, it } from "vitest";
import {
  deviceGroupNames,
  deviceIdentityHint,
  emptyDeviceOrganization,
  hasDuplicateOrganizedName,
  normalizeDeviceOrganization,
  organizedDeviceName,
  shortPeerId,
  sortDeviceGroups,
} from "./organization";

const device = {
  peerId: "12D3KooWAbCdEfGhIjKlMnOpQrStUvWxYz",
  name: "办公室 Mac",
  hostname: "macbook-pro",
};

describe("organizedDeviceName", () => {
  it("prefers local alias over the remote name", () => {
    const organization = { ...emptyDeviceOrganization, aliases: { [device.peerId]: "叶夕月的电脑" } };
    expect(organizedDeviceName(device, organization)).toBe("叶夕月的电脑");
  });

  it("falls through alias → name → hostname → short peer id", () => {
    expect(organizedDeviceName(device, emptyDeviceOrganization)).toBe("办公室 Mac");
    expect(organizedDeviceName({ ...device, name: "" }, emptyDeviceOrganization)).toBe("macbook-pro");
    expect(
      organizedDeviceName({ peerId: "abc", name: "", hostname: "" }, emptyDeviceOrganization),
    ).toBe("abc");
  });

  it("ignores whitespace-only aliases and names", () => {
    const organization = { ...emptyDeviceOrganization, aliases: { [device.peerId]: "   " } };
    expect(organizedDeviceName(device, organization)).toBe("办公室 Mac");
  });
});

describe("shortPeerId", () => {
  it("elides the middle of a long peer id", () => {
    expect(shortPeerId(device.peerId)).toBe("12D3…UvWxYz");
  });

  it("leaves short ids untouched", () => {
    expect(shortPeerId("abc")).toBe("abc");
  });
});

describe("deviceIdentityHint", () => {
  it("pairs hostname with the short peer id", () => {
    expect(deviceIdentityHint(device)).toBe("macbook-pro · 12D3…UvWxYz");
  });

  // 收口时收敛的分叉之一：桌面原样返回硬编码的「未知设备 · 短ID」（i18n-free 模块里的中文串），
  // 移动原样返回「短ID · 短ID」（同一个串说两遍）。两者在 hostname 为空时都是坏的。
  it("degrades to the short peer id alone when hostname is missing", () => {
    expect(deviceIdentityHint({ ...device, hostname: "" })).toBe("12D3…UvWxYz");
  });
});

describe("sortDeviceGroups", () => {
  it("orders by sortOrder", () => {
    const groups = [
      { id: "work", name: "工作", sortOrder: 1 },
      { id: "family", name: "家人", sortOrder: 0 },
    ];
    expect(sortDeviceGroups(groups).map((g) => g.id)).toEqual(["family", "work"]);
  });

  // 收口时收敛的分叉之二：移动端那个独立的分组排序函数原样只比 sortOrder，并列时落到插入
  // 顺序；两端的 deviceGroupNames 则本来就带名称兜底。取带兜底的那版。
  //
  // 用拉丁字母而非汉字：`localeCompare()` 不传 locale 时走运行时默认排序规则，汉字的次序
  // 三端未必一致（见 organization.ts 头部）。这条测的是「有没有兜底」，不该被排序规则绑架。
  it("breaks sortOrder ties by name", () => {
    const groups = [
      { id: "beta", name: "beta", sortOrder: 0 },
      { id: "alpha", name: "alpha", sortOrder: 0 },
    ];
    expect(sortDeviceGroups(groups).map((g) => g.id)).toEqual(["alpha", "beta"]);
  });

  it("does not mutate the input", () => {
    const groups = [
      { id: "work", name: "工作", sortOrder: 1 },
      { id: "family", name: "家人", sortOrder: 0 },
    ];
    sortDeviceGroups(groups);
    expect(groups.map((g) => g.id)).toEqual(["work", "family"]);
  });
});

describe("deviceGroupNames", () => {
  it("returns the device's groups in sorted order", () => {
    const organization = {
      aliases: {},
      groups: [
        { id: "work", name: "工作", sortOrder: 1 },
        { id: "family", name: "家人", sortOrder: 0 },
        { id: "other", name: "其它", sortOrder: 2 },
      ],
      groupDeviceIds: { work: [device.peerId], family: [device.peerId], other: ["someone-else"] },
    };
    expect(deviceGroupNames(device.peerId, organization)).toEqual(["家人", "工作"]);
  });

  it("returns nothing for an ungrouped device", () => {
    expect(deviceGroupNames(device.peerId, emptyDeviceOrganization)).toEqual([]);
  });
});

describe("hasDuplicateOrganizedName", () => {
  it("detects two devices resolving to the same display name", () => {
    const twin = { ...device, peerId: "peer-two" };
    expect(hasDuplicateOrganizedName(device, [device, twin], emptyDeviceOrganization)).toBe(true);
  });

  it("is false when the name is unique within the given list", () => {
    const other = { peerId: "peer-two", name: "别的电脑", hostname: "other-host" };
    expect(hasDuplicateOrganizedName(device, [device, other], emptyDeviceOrganization)).toBe(false);
  });

  it("counts by resolved name, so an alias can disambiguate a collision", () => {
    const twin = { ...device, peerId: "peer-two" };
    const organization = { ...emptyDeviceOrganization, aliases: { "peer-two": "楼下那台" } };
    expect(hasDuplicateOrganizedName(device, [device, twin], organization)).toBe(false);
  });
});

describe("normalizeDeviceOrganization", () => {
  it("degrades any non-object to an empty organization", () => {
    for (const value of [undefined, null, 0, "", [], "nonsense"]) {
      expect(normalizeDeviceOrganization(value)).toEqual(emptyDeviceOrganization);
    }
  });

  it("drops malformed groups and keeps well-formed ones", () => {
    const result = normalizeDeviceOrganization({
      groups: [
        { id: "ok", name: "可用", sortOrder: 3 },
        { id: "no-name" },
        null,
        "nonsense",
      ],
    });
    expect(result.groups).toEqual([{ id: "ok", name: "可用", sortOrder: 3 }]);
  });

  it("backfills a missing sortOrder from the array position", () => {
    const result = normalizeDeviceOrganization({
      groups: [
        { id: "first", name: "一" },
        { id: "second", name: "二" },
      ],
    });
    expect(result.groups.map((g) => g.sortOrder)).toEqual([0, 1]);
  });

  it("drops blank aliases", () => {
    const result = normalizeDeviceOrganization({
      aliases: { keep: "保留", blank: "   ", wrongType: 42 },
    });
    expect(result.aliases).toEqual({ keep: "保留" });
  });

  it("drops membership pointing at groups that no longer exist", () => {
    const result = normalizeDeviceOrganization({
      groups: [{ id: "live", name: "在", sortOrder: 0 }],
      groupDeviceIds: { live: ["peer-a"], dangling: ["peer-b"] },
    });
    expect(result.groupDeviceIds).toEqual({ live: ["peer-a"] });
  });

  it("drops non-string peer ids inside a membership list", () => {
    const result = normalizeDeviceOrganization({
      groups: [{ id: "live", name: "在", sortOrder: 0 }],
      groupDeviceIds: { live: ["peer-a", 7, null] },
    });
    expect(result.groupDeviceIds.live).toEqual(["peer-a"]);
  });
});
