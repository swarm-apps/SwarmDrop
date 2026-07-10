import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/tauri-store", () => ({
  createTauriStorage: () => ({
    getItem: vi.fn(async () => null),
    setItem: vi.fn(async () => undefined),
    removeItem: vi.fn(async () => undefined),
  }),
}));

import { usePreferencesStore } from "./preferences-store";

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
