import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/tauri-store", () => ({
  createTauriStorage: () => ({
    getItem: vi.fn(async () => null),
    setItem: vi.fn(async () => undefined),
    removeItem: vi.fn(async () => undefined),
  }),
}));

import {
  normalizeDeviceOrganization,
  usePreferencesStore,
} from "./preferences-store";
import { emptyDeviceOrganization } from "@/lib/device-organization";

describe("file browser preferences", () => {
  beforeEach(() => {
    usePreferencesStore.setState({
      fileBrowserViews: { send: "tree", inbox: "grid", transfer: "tree" },
    });
  });

  it("uses scene-specific defaults", () => {
    expect(usePreferencesStore.getState().fileBrowserViews).toEqual({
      send: "tree",
      inbox: "grid",
      transfer: "tree",
    });
  });

  it("updates one scene without changing the others", () => {
    usePreferencesStore.getState().setFileBrowserView("send", "grid");
    expect(usePreferencesStore.getState().fileBrowserViews).toEqual({
      send: "grid",
      inbox: "grid",
      transfer: "tree",
    });
  });
});

describe("device organization preferences", () => {
  beforeEach(() => {
    usePreferencesStore.setState({
      deviceOrganization: structuredClone(emptyDeviceOrganization),
    });
  });

  it("uses an empty organization for old preferences without organization data", () => {
    expect(normalizeDeviceOrganization(undefined)).toEqual(emptyDeviceOrganization);
  });

  it("clears aliases and supports multiple groups for one device", () => {
    const store = usePreferencesStore.getState();
    store.setDeviceAlias("peer-a", "笔记本");
    const work = store.createDeviceGroup("工作");
    const family = store.createDeviceGroup("家人");
    expect(work).not.toBeNull();
    expect(family).not.toBeNull();

    store.setDeviceGroups("peer-a", [work!, family!]);
    store.setDeviceAlias("peer-a", " ");

    expect(usePreferencesStore.getState().deviceOrganization).toMatchObject({
      aliases: {},
      groupDeviceIds: { [work!]: ["peer-a"], [family!]: ["peer-a"] },
    });
  });

  it("removes only the deleted group and cleans organization on unpair", () => {
    const store = usePreferencesStore.getState();
    const work = store.createDeviceGroup("工作")!;
    const family = store.createDeviceGroup("家人")!;
    store.setDeviceAlias("peer-a", "笔记本");
    store.setDeviceGroups("peer-a", [work, family]);
    store.deleteDeviceGroup(work);

    expect(usePreferencesStore.getState().deviceOrganization.groupDeviceIds).toEqual({
      [family]: ["peer-a"],
    });

    usePreferencesStore.getState().clearDeviceOrganization("peer-a");
    expect(usePreferencesStore.getState().deviceOrganization).toMatchObject({
      aliases: {},
      groupDeviceIds: { [family]: [] },
    });
  });

  it("preserves the reordered group order after deleting another group", () => {
    const store = usePreferencesStore.getState();
    const work = store.createDeviceGroup("工作")!;
    const home = store.createDeviceGroup("家庭")!;
    const travel = store.createDeviceGroup("出行")!;

    // 把「出行」移到最前：数组仍是插入序 [work, home, travel]，但 sortOrder 变为
    // travel=0, work=1, home=2。
    usePreferencesStore.getState().reorderDeviceGroups([travel, work, home]);
    // 删掉中间的「家庭」，剩余顺序应保持 [出行, 工作]，而非退回插入序 [工作, 出行]。
    usePreferencesStore.getState().deleteDeviceGroup(home);

    const groups = [...usePreferencesStore.getState().deviceOrganization.groups]
      .sort((a, b) => a.sortOrder - b.sortOrder)
      .map((group) => group.name);
    expect(groups).toEqual(["出行", "工作"]);
  });
});
