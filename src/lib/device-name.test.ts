import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  // `set_device_name` 返回后端归一化后真正落库的值（清空时 null），调用方据此写镜像。
  setDeviceName: vi.fn<(name: string | null) => Promise<string | null>>(),
  getDeviceName: vi.fn<() => Promise<string | null>>(),
  deviceRenamedListen: vi.fn(),
}));

vi.mock("@/lib/bindings", () => ({
  commands: {
    setDeviceName: mocks.setDeviceName,
    getDeviceName: mocks.getDeviceName,
    setLocale: vi.fn(),
  },
  events: {
    deviceRenamed: { listen: mocks.deviceRenamedListen },
  },
}));

vi.mock("@/lib/tauri-store", () => ({
  createTauriStorage: () => {
    const values = new Map<string, string>();
    return {
      getItem: async (key: string) => values.get(key) ?? null,
      setItem: async (key: string, value: string) => {
        values.set(key, value);
      },
      removeItem: async (key: string) => {
        values.delete(key);
      },
    };
  },
}));

vi.mock("@/lib/i18n", () => ({
  defaultLocale: "zh",
  dynamicActivate: vi.fn(),
  locales: { zh: "简体中文" },
}));

import {
  applyDeviceName,
  cleanupDeviceNameListener,
  setupDeviceNameListener,
} from "@/lib/device-name";
import { usePreferencesStore } from "@/stores/preferences-store";

/** `device-renamed` 的 payload 形状（与 bindings 的 `DeviceRenamed` 一致）。 */
interface RenamedPayload {
  name: string | null;
  displayName: string;
}

describe("applyDeviceName", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.setDeviceName.mockResolvedValue("书房 Mac");
    usePreferencesStore.setState({ deviceName: "" });
  });

  // 回归锚点：改名此前要前端自己 stopNetwork + startNetwork，代价是断开所有连接、
  // 打断在途传输。现在后端逐连接下发 agent_version，前端一次命令就够了。
  it("只调一次命令，不再碰节点生命周期", async () => {
    await expect(applyDeviceName("书房 Mac")).resolves.toBeUndefined();

    expect(mocks.setDeviceName).toHaveBeenCalledExactlyOnceWith("书房 Mac");
    expect(usePreferencesStore.getState().deviceName).toBe("书房 Mac");
  });

  it("空白输入清空设备名（传 null 回退 hostname）", async () => {
    mocks.setDeviceName.mockResolvedValue(null);

    await applyDeviceName("   ");

    expect(mocks.setDeviceName).toHaveBeenCalledWith(null);
    expect(usePreferencesStore.getState().deviceName).toBe("");
  });

  it("镜像回写后端归一化后的结果，不是用户原样输入", async () => {
    // 后端剥掉了 `;`（agent_version 分隔符注入）
    mocks.setDeviceName.mockResolvedValue("我的电脑 caps=lan-helper");

    await applyDeviceName("我的电脑; caps=lan-helper");

    expect(mocks.setDeviceName).toHaveBeenCalledWith("我的电脑; caps=lan-helper");
    expect(usePreferencesStore.getState().deviceName).toBe(
      "我的电脑 caps=lan-helper",
    );
  });

  // 回归锚点：`setDeviceName` 曾返回 void，前端只能再 `getDeviceName()` 回读一次才知道
  // 落库值。现在它直接返回归一化结果，回读那一趟 IPC 必须消失——留着不只是多一次往返，
  // 两次调用之间另一个窗口/MCP 改了名，回读拿到的就是别人的值。
  it("归一化值直接取自 setDeviceName 返回值，没有第二次 IPC", async () => {
    mocks.setDeviceName.mockResolvedValue("书房 Mac");
    // 若真回读了，这个值会覆盖掉上面的返回值，断言随即失败
    mocks.getDeviceName.mockResolvedValue("别的窗口刚改的名");

    await applyDeviceName("书房 Mac   ");

    expect(mocks.getDeviceName).not.toHaveBeenCalled();
    expect(usePreferencesStore.getState().deviceName).toBe("书房 Mac");
  });

  it("保存失败直接抛出，不吞成成功", async () => {
    mocks.setDeviceName.mockRejectedValue(new Error("写盘失败"));

    await expect(applyDeviceName("书房 Mac")).rejects.toThrow("写盘失败");
  });
});

describe("setupDeviceNameListener", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePreferencesStore.setState({ deviceName: "" });
  });

  it("外部来源改名（另一个窗口 / MCP）也能同步到镜像", async () => {
    let handler!: (event: { payload: RenamedPayload }) => void;
    const unlisten = vi.fn();
    mocks.deviceRenamedListen.mockImplementation(
      async (cb: (event: { payload: RenamedPayload }) => void) => {
        handler = cb;
        return unlisten;
      },
    );

    await setupDeviceNameListener();
    handler({ payload: { name: "书房 Mac", displayName: "书房 Mac" } });
    expect(usePreferencesStore.getState().deviceName).toBe("书房 Mac");

    // 清空时 displayName 回退成 hostname，但镜像必须落回空串 —— 存 displayName 会让
    // 用户看起来「设过一个恰好等于机器名的名字」，再点编辑就把它固化成真名字了。
    handler({ payload: { name: null, displayName: "MacBook-Pro" } });
    expect(usePreferencesStore.getState().deviceName).toBe("");

    cleanupDeviceNameListener();
    expect(unlisten).toHaveBeenCalledOnce();
  });
});
