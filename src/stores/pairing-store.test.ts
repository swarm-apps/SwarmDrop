/**
 * 邀请撤销的回归守卫。
 *
 * 邀请是一次性信任凭证：UI 上被顶掉/清空后，后端 registry 里也必须立刻作废，
 * 否则那串旧邀请还能一直用到 TTL 到点（300s）——而界面已经承诺它失效了。
 * 这条链路全是 fire-and-forget，断了不会有任何报错，只能靠测试钉住。
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  generatePairInvite: vi.fn(),
  revokePairInvite: vi.fn().mockResolvedValue(null),
}));

vi.mock("@/lib/bindings", () => ({
  commands: {
    generatePairInvite: mocks.generatePairInvite,
    revokePairInvite: mocks.revokePairInvite,
    decodePairInvite: vi.fn(),
    consumePairInvite: vi.fn(),
  },
}));

vi.mock("@/stores/network-store", () => ({
  findNetworkDeviceSnapshot: vi.fn(),
  startNetworkFromStore: vi.fn(),
}));

vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

import { usePairingStore } from "@/stores/pairing-store";

describe("pairing-store 邀请撤销", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.revokePairInvite.mockResolvedValue(null);
    usePairingStore.setState({ activeInvite: null, inviteError: null });
  });

  it("重新生成时撤销被覆盖的旧邀请", async () => {
    mocks.generatePairInvite
      .mockResolvedValueOnce("sd:first")
      .mockResolvedValueOnce("sd:second");

    await usePairingStore.getState().generateInvite(false);
    expect(mocks.revokePairInvite).not.toHaveBeenCalled();

    await usePairingStore.getState().generateInvite(true);
    expect(mocks.revokePairInvite).toHaveBeenCalledExactlyOnceWith("sd:first");
    expect(usePairingStore.getState().activeInvite?.invite).toBe("sd:second");
  });

  it("清空活跃邀请时一并撤销", async () => {
    mocks.generatePairInvite.mockResolvedValue("sd:only");
    await usePairingStore.getState().generateInvite(false);

    usePairingStore.getState().clearActiveInvite();

    expect(mocks.revokePairInvite).toHaveBeenCalledExactlyOnceWith("sd:only");
    expect(usePairingStore.getState().activeInvite).toBeNull();
  });

  it("生成失败也要撤销旧邀请——界面已经把它作废了", async () => {
    mocks.generatePairInvite.mockResolvedValueOnce("sd:first");
    await usePairingStore.getState().generateInvite(false);

    mocks.generatePairInvite.mockRejectedValueOnce(new Error("boom"));
    await usePairingStore.getState().generateInvite(false);

    expect(mocks.revokePairInvite).toHaveBeenCalledExactlyOnceWith("sd:first");
    expect(usePairingStore.getState().activeInvite).toBeNull();
  });

  it("撤销失败不外溢——fire-and-forget 不该打断界面", async () => {
    mocks.generatePairInvite.mockResolvedValue("sd:only");
    await usePairingStore.getState().generateInvite(false);
    mocks.revokePairInvite.mockRejectedValue(new Error("node stopped"));

    expect(() => usePairingStore.getState().clearActiveInvite()).not.toThrow();
    expect(usePairingStore.getState().activeInvite).toBeNull();
  });

  it("没有活跃邀请时不发无谓的撤销请求", async () => {
    usePairingStore.getState().clearActiveInvite();
    expect(mocks.revokePairInvite).not.toHaveBeenCalled();
  });
});
