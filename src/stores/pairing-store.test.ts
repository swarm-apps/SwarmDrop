/**
 * 邀请撤销的回归守卫。
 *
 * 邀请是一次性信任凭证：UI 上被顶掉/清空后，后端 registry 里也必须立刻作废，
 * 否则那串旧邀请还能一直用到 TTL 到点（24h，invite-persistence 起从 300s 放宽）——
 * 而界面已经承诺它失效了。
 * 这条链路全是 fire-and-forget，断了不会有任何报错，只能靠测试钉住。
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

/** 邀请串在本文件里是不透明的 —— store 不解析它，只透传给后端。
 *  用 canonical 形态而不是随手写的假串：`sd:` 是已废弃的旧 scheme（invite-url-canonical
 *  删掉了它的读取路径），留在 fixture 里会教给读者一个不存在的 wire 形态。 */
const INVITE_A = "https://swarm-apps.github.io/SwarmDrop/p/#AAAAAAAA";
const INVITE_B = "https://swarm-apps.github.io/SwarmDrop/p/#BBBBBBBB";

const mocks = vi.hoisted(() => ({
  generatePairInvite: vi.fn(),
  revokePairInvite: vi.fn().mockResolvedValue(true),
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
      .mockResolvedValueOnce(INVITE_A)
      .mockResolvedValueOnce(INVITE_B);

    await usePairingStore.getState().generateInvite(false);
    expect(mocks.revokePairInvite).not.toHaveBeenCalled();

    await usePairingStore.getState().generateInvite(true);
    expect(mocks.revokePairInvite).toHaveBeenCalledExactlyOnceWith(INVITE_A);
    expect(usePairingStore.getState().activeInvite?.invite).toBe(INVITE_B);
  });

  it("清空活跃邀请时一并撤销", async () => {
    mocks.generatePairInvite.mockResolvedValue(INVITE_A);
    await usePairingStore.getState().generateInvite(false);

    usePairingStore.getState().clearActiveInvite();

    expect(mocks.revokePairInvite).toHaveBeenCalledExactlyOnceWith(INVITE_A);
    expect(usePairingStore.getState().activeInvite).toBeNull();
  });

  it("生成失败也要撤销旧邀请——界面已经把它作废了", async () => {
    mocks.generatePairInvite.mockResolvedValueOnce(INVITE_A);
    await usePairingStore.getState().generateInvite(false);

    mocks.generatePairInvite.mockRejectedValueOnce(new Error("boom"));
    await usePairingStore.getState().generateInvite(false);

    expect(mocks.revokePairInvite).toHaveBeenCalledExactlyOnceWith(INVITE_A);
    expect(usePairingStore.getState().activeInvite).toBeNull();
  });

  it("撤销失败不外溢——fire-and-forget 不该打断界面", async () => {
    mocks.generatePairInvite.mockResolvedValue(INVITE_A);
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
