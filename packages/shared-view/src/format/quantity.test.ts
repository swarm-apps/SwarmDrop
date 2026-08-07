import { describe, expect, it } from "vitest";
import { calcPercent, formatFileSize, formatTransferRate } from "./quantity";

describe("formatFileSize", () => {
  it("uses whole bytes below 1 KiB", () => {
    expect(formatFileSize(0)).toBe("0 B");
    expect(formatFileSize(1023)).toBe("1023 B");
  });

  it("switches unit at each 1024 boundary", () => {
    expect(formatFileSize(1024)).toBe("1.0 KB");
    expect(formatFileSize(1024 * 1024)).toBe("1.0 MB");
    expect(formatFileSize(1024 * 1024 * 1024)).toBe("1.00 GB");
  });

  it("accepts bigint (the shape uniffi hands the mobile end)", () => {
    expect(formatFileSize(1024n)).toBe("1.0 KB");
    expect(formatFileSize(5n * 1024n * 1024n * 1024n)).toBe("5.00 GB");
  });
});

describe("formatTransferRate", () => {
  it("appends /s to the byte size", () => {
    expect(formatTransferRate(1024)).toBe("1.0 KB/s");
  });

  // 占位文案是各端 i18n 的事，本函数只说「算不出来」。
  it("returns null instead of a placeholder when the rate is unknown", () => {
    expect(formatTransferRate(null)).toBeNull();
    expect(formatTransferRate(undefined)).toBeNull();
    expect(formatTransferRate(Number.NaN)).toBeNull();
    expect(formatTransferRate(Number.POSITIVE_INFINITY)).toBeNull();
    expect(formatTransferRate(0)).toBeNull();
    expect(formatTransferRate(-1)).toBeNull();
  });
});

describe("calcPercent", () => {
  it("rounds to whole percent", () => {
    expect(calcPercent(1, 3)).toBe(33);
    expect(calcPercent(2, 3)).toBe(67);
  });

  it("returns 0 for a non-positive total instead of dividing by zero", () => {
    expect(calcPercent(5, 0)).toBe(0);
    expect(calcPercent(5, -1)).toBe(0);
  });

  // 收口时收敛的分叉之三：桌面与 Web 原样没有夹取，统计出错时能渲染出 137% 的进度条。
  it("clamps overshoot to 100", () => {
    expect(calcPercent(150, 100)).toBe(100);
  });

  it("accepts bigint on either side", () => {
    expect(calcPercent(512n, 1024n)).toBe(50);
  });
});
