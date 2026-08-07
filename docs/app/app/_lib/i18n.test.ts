import { describe, expect, it } from "vitest";

import { preferredLocale } from "./i18n";

/**
 * `preferredLocale` 的错法全是**静默**的：匹配到一个受支持但不对的 locale，界面照常渲染，
 * 只是字形或语言不对。没有报错、没有 fallback 日志，所以只能靠用例钉住。
 *
 * 同一组规则在配对落地页（`docs/public/p/index.html`）里有一份手写副本 —— 那页不经构建、
 * 无法 import 本模块。两边**的判定结果**必须一致，改这里就要改那里（那边的 `STRINGS`
 * 上方注释有指路）。
 */
describe("preferredLocale", () => {
  it("精确匹配优先", () => {
    expect(preferredLocale(["zh-TW"])).toBe("zh-TW");
    expect(preferredLocale(["en"])).toBe("en");
    expect(preferredLocale(["zh"])).toBe("zh");
  });

  it("繁体变体归 zh-TW，不归简体", () => {
    // 这几条是本用例存在的理由：按主语言归类会把它们全判成简体 `zh`，
    // 香港用户于是拿到一屏简体字 —— 匹配「成功」了，只是给错了字形。
    expect(preferredLocale(["zh-HK"])).toBe("zh-TW");
    expect(preferredLocale(["zh-MO"])).toBe("zh-TW");
    expect(preferredLocale(["zh-Hant"])).toBe("zh-TW");
    expect(preferredLocale(["zh-Hant-HK"])).toBe("zh-TW");

    // **Chrome 会把基语言补进列表**，港澳用户实际发的是这个形状。单元素用例挡不住它：
    // 只要实现敢跨 tag 先扫一遍精确匹配，末尾那个裸 `zh` 就会抢先命中并给出简体。
    expect(preferredLocale(["zh-HK", "zh"])).toBe("zh-TW");
    expect(preferredLocale(["zh-MO", "zh"])).toBe("zh-TW");
    expect(preferredLocale(["zh-Hant-HK", "zh", "en-US", "en"])).toBe("zh-TW");
  });

  it("显式声明的简体不被地区码带偏", () => {
    // `-hk` / `-mo` 是地区不是字形。`zh-Hans-HK` 明说了 Hans，判据里 hans 必须先于地区码。
    expect(preferredLocale(["zh-Hans-HK"])).toBe("zh");
    expect(preferredLocale(["zh-Hans-MO"])).toBe("zh");
  });

  it("粤语按繁体处理", () => {
    // `yue` 是 Chrome 语言列表里的「粵語」。不认它就会落进主语言轮或源 locale，
    // 结果又是给港澳用户一屏简体字 —— 与上面那条是同一个失败模式。
    expect(preferredLocale(["yue-HK"])).toBe("zh-TW");
    expect(preferredLocale(["yue"])).toBe("zh-TW");
  });

  it("简体变体归 zh", () => {
    expect(preferredLocale(["zh-CN"])).toBe("zh");
    expect(preferredLocale(["zh-SG"])).toBe("zh");
    expect(preferredLocale(["zh-Hans-CN"])).toBe("zh");
  });

  it("按整个偏好列表退让，不是只看首选", () => {
    // 用户首选法语、次选繁中：法语不支持，就该落到他的第二偏好，而不是直接回退源 locale。
    expect(preferredLocale(["fr-FR", "zh-TW"])).toBe("zh-TW");
    expect(preferredLocale(["ja", "en-US"])).toBe("en");
  });

  it("偏好顺序压过匹配精度", () => {
    // 这一组是本文件最容易写错的地方：曾经的实现先把整个列表精确扫一遍、再扫主语言，
    // 于是靠后但精确的 tag 压过靠前但不精确的 —— 首选简体中文的用户拿到英文界面。
    // `["zh-CN","en-US","en"]` 是 Chrome 上中文用户最常见的列表之一。
    expect(preferredLocale(["zh-CN", "en-US", "en"])).toBe("zh");
    expect(preferredLocale(["en-US", "zh-TW"])).toBe("en");
    expect(preferredLocale(["en-GB", "zh"])).toBe("en");
    expect(preferredLocale(["zh-CN", "zh-TW"])).toBe("zh");
  });

  it("英语变体归 en", () => {
    expect(preferredLocale(["en-US"])).toBe("en");
    expect(preferredLocale(["en-GB"])).toBe("en");
  });

  it("一个都不沾则回退源 locale", () => {
    expect(preferredLocale(["fr-FR"])).toBe("zh");
    expect(preferredLocale([])).toBe("zh");
  });
});
