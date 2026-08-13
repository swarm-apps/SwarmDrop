import { describe, expect, it, vi } from "vitest";

import { createSessionTimers } from "./session-timers";

/** 注入一个可手动推进的假调度器——本包没有 `setTimeout`，也不该依赖它。 */
function fakeScheduler() {
  const pending = new Map<number, () => void>();
  let nextId = 1;
  return {
    setTimer: (fire: () => void) => {
      const id = nextId++;
      pending.set(id, fire);
      return id;
    },
    clearTimer: (id: number) => {
      pending.delete(id);
    },
    fireAll() {
      for (const [id, fire] of [...pending]) {
        pending.delete(id);
        fire();
      }
    },
    get liveCount() {
      return pending.size;
    },
  };
}

describe("createSessionTimers", () => {
  it("keeps at most one timer per session", () => {
    const clock = fakeScheduler();
    const timers = createSessionTimers(clock.setTimer, clock.clearTimer);
    const fire = vi.fn();

    timers.schedule("a", 100, fire);
    timers.schedule("a", 100, fire);

    expect(timers.size).toBe(1);
    expect(clock.liveCount).toBe(1);
    clock.fireAll();
    expect(fire).toHaveBeenCalledTimes(1);
  });

  // 陈旧老化那条回调会当场重排一条；先摘句柄再执行，否则新排的会被当成旧的删掉。
  it("survives a callback that reschedules itself", () => {
    const clock = fakeScheduler();
    const timers = createSessionTimers(clock.setTimer, clock.clearTimer);

    timers.schedule("a", 100, () => {
      timers.schedule("a", 100, () => {});
    });
    clock.fireAll();

    expect(timers.size).toBe(1);
    expect(clock.liveCount).toBe(1);
  });

  it("reports whether a cancel actually removed something", () => {
    const clock = fakeScheduler();
    const timers = createSessionTimers(clock.setTimer, clock.clearTimer);

    timers.schedule("a", 100, () => {});
    expect(timers.cancel("a")).toBe(true);
    expect(timers.cancel("a")).toBe(false);
    expect(timers.size).toBe(0);
  });

  it("drops everything the live check rejects", () => {
    const clock = fakeScheduler();
    const timers = createSessionTimers(clock.setTimer, clock.clearTimer);
    const dropped = vi.fn();

    timers.schedule("keep", 100, () => {});
    timers.schedule("drop", 100, dropped);
    timers.retain((id) => id === "keep");

    expect(timers.size).toBe(1);
    clock.fireAll();
    expect(dropped).not.toHaveBeenCalled();
  });

  it("clears the whole ledger", () => {
    const clock = fakeScheduler();
    const timers = createSessionTimers(clock.setTimer, clock.clearTimer);

    timers.schedule("a", 100, () => {});
    timers.schedule("b", 100, () => {});
    timers.clear();

    expect(timers.size).toBe(0);
    expect(clock.liveCount).toBe(0);
  });
});
