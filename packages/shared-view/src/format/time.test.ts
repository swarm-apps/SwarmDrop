import { describe, expect, it } from "vitest";
import { formatDuration, formatEta, formatLatency, formatTimeLeft } from "./time";

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

describe("formatEta", () => {
  // 「算不出来」和「还剩 0 秒」是两件事：前者要占位文案，后者是一个真实的数字。
  // formatDuration 把两者都压成 "0s"，所以 ETA 不能直接用它。
  it("returns null when it cannot be computed", () => {
    expect(formatEta(null)).toBeNull();
    expect(formatEta(undefined)).toBeNull();
    expect(formatEta(Number.NaN)).toBeNull();
    expect(formatEta(Number.POSITIVE_INFINITY)).toBeNull();
    expect(formatEta(-1)).toBeNull();
  });

  // 后端的 eta 是 3 秒滑窗速率的直接商，逐帧会跳字；粗化让秒位不再闪。
  it("quantises to five-second steps below a minute", () => {
    expect(formatEta(0)).toBe("0s");
    expect(formatEta(1)).toBe("5s");
    expect(formatEta(5)).toBe("5s");
    expect(formatEta(6)).toBe("10s");
    expect(formatEta(43)).toBe("45s");
  });

  it("quantises to ten-second steps from a minute up", () => {
    expect(formatEta(59)).toBe("1m 0s");
    expect(formatEta(61)).toBe("1m 10s");
    expect(formatEta(200)).toBe("3m 20s");
  });

  // 向上取整：报少了会出现「说完了却还在跑」。
  // 样本全部取在 1 小时以内——下面的反解只认 `Ns` 与 `Nm Ns` 两种形态，
  // 一小时以上 formatDuration 会切到 `Nh Nm` 而丢掉秒位。
  it("never reports less time than remains", () => {
    for (const seconds of [1, 7, 33, 58, 61, 119, 3500]) {
      const text = formatEta(seconds)!;
      const [, minutes = "0", secs] = /^(?:(\d+)m )?(\d+)s$/.exec(text)!;
      expect(Number(minutes) * 60 + Number(secs)).toBeGreaterThanOrEqual(seconds);
    }
  });
});
