import { describe, expect, it } from "vitest";

import { isProgressFresh, PROGRESS_STALE_MS, usableEta } from "./progress";

const NOW = 1_700_000_000_000;

describe("isProgressFresh", () => {
  it("treats a missing arrival time as stale", () => {
    expect(isProgressFresh(null, NOW)).toBe(false);
    expect(isProgressFresh(undefined, NOW)).toBe(false);
    expect(isProgressFresh(Number.NaN, NOW)).toBe(false);
  });

  it("goes stale exactly at the window", () => {
    expect(isProgressFresh(NOW - (PROGRESS_STALE_MS - 1), NOW)).toBe(true);
    expect(isProgressFresh(NOW - PROGRESS_STALE_MS, NOW)).toBe(false);
  });
});

describe("usableEta", () => {
  // 停滞时不发新帧，最后一帧会永远躺在 store 里——不判时效的话
  // 「剩余 45s」会一直显示到会话超时为止。
  it("drops the eta of a stale frame", () => {
    expect(usableEta(45, NOW - 1_000, NOW)).toBe(45);
    expect(usableEta(45, NOW - PROGRESS_STALE_MS, NOW)).toBeNull();
  });

  it("normalises a fresh frame with no eta to null", () => {
    expect(usableEta(null, NOW, NOW)).toBeNull();
    expect(usableEta(undefined, NOW, NOW)).toBeNull();
  });
});
