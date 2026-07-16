/**
 * 更新弹窗的渲染层接线 —— 上游 registry 用纯函数测了「谁该可见」的不变量,但测不到组件有没有
 * 真的照着用(它无 DOM render 设施)。本文件补的就是这段:同一棵树里挂上三个弹窗,断言实际
 * 渲染出来的东西。v0.7.6 的三个线上症状各对应下面一组用例。
 */
import { i18n } from "@lingui/core";
import { cleanup, render, screen } from "@testing-library/react";
import type { ReleaseInfo, UpdateEngineState, UpdateStatus, UpgradeType } from "@swarm-hive/sdk";
import { afterEach, describe, expect, it, vi } from "vitest";

const updateState = vi.hoisted(() => ({ current: null as unknown as UpdateEngineState }));

vi.mock("@/hooks/use-update", () => ({
  useUpdate: () => updateState.current,
}));

import { ForceUpdateDialog } from "@/components/force-update-dialog";
import { PromptUpdateDialog } from "@/components/prompt-update-dialog";
import { UpdateProgressDialog } from "@/components/update-progress-dialog";
import { progressDialogVisible } from "@/lib/update-dialog-visibility";

const NOTES = "## Bug Fixes\n\n- **mobile:** 修好了 [#12](https://example.test/pr/12)\n";

function setEngine(status: UpdateStatus, upgradeType: UpgradeType) {
  const release: ReleaseInfo = {
    version: "0.7.7",
    url: "https://example.test/download/swarmdrop/0.7.7/dmg",
    upgradeType,
    channel: "stable",
    notes: NOTES,
  };
  updateState.current = {
    status,
    release,
    progress: status === "downloading" ? { percent: 0.21, downloaded: 21, total: 100 } : null,
    error: null,
    check: vi.fn(),
    download: vi.fn(),
    install: vi.fn(),
    postpone: vi.fn(),
    acknowledgeError: vi.fn(),
  } as unknown as UpdateEngineState;
}

/** 复刻 __root.tsx 的 UpdateGate 编排：prompt 下载中保持打开，progress 仅作兜底。 */
function mountAll(promptOpen: boolean) {
  const { status, release } = updateState.current;
  return render(
    <>
      <ForceUpdateDialog />
      <PromptUpdateDialog open={promptOpen} onOpenChange={vi.fn()} />
      <UpdateProgressDialog open={!promptOpen && progressDialogVisible(status, release)} />
    </>,
  );
}

afterEach(() => {
  cleanup();
  i18n.activate("zh");
});

describe("普通更新（upgradeType = prompt）", () => {
  // 线上症状 1：force 弹窗据 status 推导强制性，downloading 一到就冒名顶替「需要更新」，
  // 不可关地叠在 prompt 之上，把用户锁到下载结束。
  it("下载中不弹出强制更新弹窗", () => {
    setEngine("downloading", "prompt");
    mountAll(true);

    expect(screen.queryByText("需要更新")).toBeNull();
    expect(screen.getByText("发现新版本")).toBeTruthy();
  });

  // 线上症状 2：两个弹窗同框时，上层的 modal overlay 吃掉下层 release notes 的滚动与点击。
  it("下载中全程只有一个弹窗", () => {
    setEngine("downloading", "prompt");
    mountAll(true);

    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    // prompt 自带内联进度，故不需要第二个弹窗来承载。
    expect(screen.getByText("21%")).toBeTruthy();
    expect(screen.queryByText("正在下载更新")).toBeNull();
  });

  it("用户关掉 prompt 后，由兜底进度弹窗接管", () => {
    setEngine("downloading", "prompt");
    mountAll(false);

    const dialogs = screen.getAllByRole("dialog");
    expect(dialogs).toHaveLength(1);
    expect(screen.getByText("正在下载更新")).toBeTruthy();
  });
});

describe("强制更新（upgradeType = force）", () => {
  it("全程由 force 弹窗独占，进度弹窗让位", () => {
    setEngine("downloading", "force");
    mountAll(false);

    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(screen.getByText("需要更新")).toBeTruthy();
    expect(screen.queryByText("正在下载更新")).toBeNull();
  });
});

describe("文案本地化", () => {
  // 线上症状 3：挂载点没传 locale，而 registry 版 resolveUpdateTexts 缺省硬编码 "en"
  // → 整套弹窗对中文用户显示英文。弹窗不该依赖挂载点记得传 locale。
  it("挂载时不传 locale 也跟随应用语言", () => {
    setEngine("available", "prompt");
    mountAll(true);

    expect(screen.getByText("发现新版本")).toBeTruthy();
    expect(screen.getByText("立即更新")).toBeTruthy();
    expect(screen.getByText("稍后提醒")).toBeTruthy();
    expect(screen.queryByText("Update available")).toBeNull();
  });

  it("切到 en 时用英文", () => {
    i18n.activate("en");
    setEngine("available", "prompt");
    mountAll(true);

    expect(screen.getByText("Update available")).toBeTruthy();
    expect(screen.getByText("Update now")).toBeTruthy();
  });
});

describe("release notes 渲染", () => {
  // 线上症状 4（v0.7.6 已发布版本）：release notes 按纯文本渲染，用户看到的是原始
  // markdown 语法（## / ** / [](url)）。修法已在 develop（ba48a3b），此处钉住不回退。
  it("渲染为 markdown 而非原始语法", () => {
    setEngine("available", "prompt");
    mountAll(true);

    // 标题降级成 h3/h4，但文字本身不该带 "##"。
    expect(screen.getByText("Bug Fixes").tagName).toMatch(/^H[1-6]$/);
    expect(screen.getByText("mobile:").tagName).toBe("STRONG");

    const link = screen.getByRole("link", { name: "#12" });
    expect(link.getAttribute("href")).toBe("https://example.test/pr/12");

    expect(screen.queryByText(/##|\*\*/)).toBeNull();
  });
});
