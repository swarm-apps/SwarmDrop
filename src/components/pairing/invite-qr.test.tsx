import { i18n } from "@lingui/core";
import { I18nProvider } from "@lingui/react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { InviteQr } from "./invite-qr";

const inviteQrSvg = vi.fn();

vi.mock("@/lib/bindings", () => ({
  commands: {
    inviteQrSvg: (invite: string) => inviteQrSvg(invite),
  },
}));

afterEach(() => {
  cleanup();
  inviteQrSvg.mockReset();
});

function renderQr(ui: React.ReactElement) {
  return render(<I18nProvider i18n={i18n}>{ui}</I18nProvider>);
}

describe("InviteQr 状态呈现", () => {
  it("渲染后端返回的 SVG，并给出可读的图像标签", async () => {
    inviteQrSvg.mockResolvedValue("<svg><rect /></svg>");
    renderQr(<InviteQr invite="sdinvite1abc" />);

    const image = await screen.findByRole("img");
    expect(image.getAttribute("aria-label")).toBeTruthy();
    expect(image.querySelector("svg")).not.toBeNull();
  });

  it("过期时保留码面但压暗到扫不动，状态就地说明", async () => {
    inviteQrSvg.mockResolvedValue("<svg><rect /></svg>");

    renderQr(
      <InviteQr
        invite="sdinvite1abc"
        overlay={{ kind: "expired", message: "邀请已过期" }}
      />,
    );

    const image = await screen.findByRole("img");
    // 码没有被替换掉，只是明确失效——布局不跳、状态可见
    expect(image.className).toContain("opacity-");
    expect(screen.getByText("邀请已过期")).toBeTruthy();
    // 恢复动作归 CommandDock，码面不再自带按钮（一屏一个主动作）
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("IPC 失败时不静默留白，而是报出渲染失败", async () => {
    inviteQrSvg.mockRejectedValue(new Error("boom"));
    renderQr(<InviteQr invite="sdinvite1abc" />);

    await waitFor(() => {
      expect(screen.getByText("二维码渲染失败")).toBeTruthy();
    });
  });
});
