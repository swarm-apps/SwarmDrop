import { describe, expect, it } from "vitest";
import { formatDuration, formatLatency, formatTimeLeft } from "./time";

describe("formatDuration", () => {
  it("uses seconds below a minute, rounding up", () => {
    expect(formatDuration(0)).toBe("0s");
    expect(formatDuration(0.2)).toBe("1s");
    expect(formatDuration(59)).toBe("59s");
  });

  it("uses minutes and seconds below an hour", () => {
    expect(formatDuration(60)).toBe("1m 0s");
    expect(formatDuration(200)).toBe("3m 20s");
  });

  it("uses hours and minutes from an hour up", () => {
    expect(formatDuration(3600)).toBe("1h 0m");
    expect(formatDuration(3900)).toBe("1h 5m");
  });

  it("degrades to 0s for negative or non-finite input", () => {
    expect(formatDuration(-1)).toBe("0s");
    expect(formatDuration(Number.NaN)).toBe("0s");
  });
});

describe("formatLatency", () => {
  it("renders positive milliseconds", () => {
    expect(formatLatency(42)).toBe("42ms");
  });

  // 0ms 是取整后的占位（<1ms 的直连），显示出来像 bug——由调用方决定只显示连接类型。
  it("returns null for absent or non-positive latency", () => {
    expect(formatLatency(null)).toBeNull();
    expect(formatLatency(undefined)).toBeNull();
    expect(formatLatency(0)).toBeNull();
    expect(formatLatency(-5)).toBeNull();
  });
});

describe("formatTimeLeft", () => {
  it("switches granularity by magnitude", () => {
    expect(formatTimeLeft(30, "en")).toBe("30 seconds");
    expect(formatTimeLeft(600, "en")).toBe("10 minutes");
    expect(formatTimeLeft(86_340, "en")).toBe("24 hours");
  });

  it("follows the locale it is given", () => {
    expect(formatTimeLeft(600, "zh")).toContain("10");
    expect(formatTimeLeft(600, "zh")).not.toBe(formatTimeLeft(600, "en"));
  });

  // 方向词（「后过期」/ "Expires in") 归调用点的翻译，否则两边都说一次就成了「将在 X 后 后过期」。
  // 与 Intl.RelativeTimeFormat 对照：那个自带方向（"in 10 minutes"），这里必须没有。
  it("returns the duration alone, with no direction word", () => {
    expect(formatTimeLeft(600, "en")).toBe("10 minutes");
    expect(new Intl.RelativeTimeFormat("en").format(10, "minute")).toBe("in 10 minutes");
  });

  // 「已过期」是一句要翻译的 UI 文案，不该由格式化函数硬编码。
  it("returns an empty string once expired", () => {
    expect(formatTimeLeft(0, "en")).toBe("");
    expect(formatTimeLeft(-1, "en")).toBe("");
  });
});
