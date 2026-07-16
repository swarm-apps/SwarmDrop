import { i18n } from "@lingui/core";
import { afterEach, describe, expect, it } from "vitest";
import { resolveUpdateTexts, updateTextPresets } from "@/lib/update-texts";
import { locales } from "@/lib/i18n";

// setup.ts 全局激活 "zh"，用例改完要还原，免得泄漏给同文件后续用例。
afterEach(() => {
  i18n.activate("zh");
});

describe("resolveUpdateTexts", () => {
  // 线上回归（v0.7.6）：registry 版缺省 locale 硬编码 "en"，而挂载点（__root.tsx）没传
  // locale —— 整套更新弹窗对所有中文用户显示英文。缺省跟随活动语言才让它不可能再发生。
  it("不传 locale 时跟随 lingui 活动语言", () => {
    i18n.activate("zh");
    expect(resolveUpdateTexts().promptTitle).toBe("发现新版本");

    i18n.activate("zh-TW");
    expect(resolveUpdateTexts().promptTitle).toBe("發現新版本");

    i18n.activate("en");
    expect(resolveUpdateTexts().promptTitle).toBe("Update available");
  });

  it("活动语言不在支持列表时退回默认语言，而非英文", () => {
    i18n.activate("ja");
    expect(resolveUpdateTexts().promptTitle).toBe(updateTextPresets.zh.promptTitle);
  });

  it("显式 locale 优先于活动语言", () => {
    i18n.activate("zh");
    expect(resolveUpdateTexts("en").promptTitle).toBe("Update available");
  });

  it("overrides 覆盖预设", () => {
    i18n.activate("zh");
    expect(resolveUpdateTexts("zh", { updateButton: "马上装" }).updateButton).toBe("马上装");
  });
});

describe("updateTextPresets", () => {
  // 漏一个语言 = 该语言用户拿到 undefined 文案（Record 的类型检查挡不住运行时的动态 key）。
  it("覆盖 i18n 支持的每一种语言", () => {
    expect(Object.keys(updateTextPresets).sort()).toEqual(Object.keys(locales).sort());
  });

  it("每种语言的每个键都有非空文案", () => {
    for (const [locale, texts] of Object.entries(updateTextPresets)) {
      for (const [key, value] of Object.entries(texts)) {
        const rendered = typeof value === "function" ? value("1.0.0", "0.9.0") : value;
        expect(rendered, `${locale}.${key}`).toBeTruthy();
      }
    }
  });
});
