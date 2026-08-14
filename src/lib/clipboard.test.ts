import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  readText: vi.fn<() => Promise<string>>(),
  writeText: vi.fn<(text: string) => Promise<boolean>>(),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  readText: mocks.readText,
  writeText: mocks.writeText,
}));

import { clipboard, copyText, readText } from "@/lib/clipboard";

describe("desktop ClipboardPort", () => {
  it("所有公开读写入口都经原生适配器委托", async () => {
    mocks.readText.mockResolvedValue("验证码");
    mocks.writeText.mockResolvedValue(true);

    await expect(readText()).resolves.toBe("验证码");
    await copyText("验证码");
    await clipboard.writeText("链接");

    expect(mocks.readText).toHaveBeenCalledOnce();
    expect(mocks.writeText).toHaveBeenNthCalledWith(1, "验证码");
    expect(mocks.writeText).toHaveBeenNthCalledWith(2, "链接");
  });
});
