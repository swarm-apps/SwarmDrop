import { describe, expect, it } from "vitest";

import { compareByTimelineDesc, sortByTimelineDesc } from "./ordering";

const at = (sessionId: string, updatedAt: number | bigint) => ({ sessionId, updatedAt });

describe("sortByTimelineDesc", () => {
  it("最后活动时间倒序", () => {
    const sorted = sortByTimelineDesc([at("a", 100), at("b", 300), at("c", 200)]);

    expect(sorted.map((s) => s.sessionId)).toEqual(["b", "c", "a"]);
  });

  it("不改原数组", () => {
    const input = [at("a", 100), at("b", 300)];

    sortByTimelineDesc(input);

    expect(input.map((s) => s.sessionId)).toEqual(["a", "b"]);
  });

  it("同一毫秒按 sessionId 兜底——否则两行会随 Record 插入序换位置", () => {
    const one = sortByTimelineDesc([at("z", 100), at("a", 100)]);
    const other = sortByTimelineDesc([at("a", 100), at("z", 100)]);

    expect(one.map((s) => s.sessionId)).toEqual(["a", "z"]);
    expect(other.map((s) => s.sessionId)).toEqual(["a", "z"]);
  });

  it("number 与 bigint 混用时仍是合法比较器（反对称）——错了会静默给出任意顺序", () => {
    const bigintSide = at("a", 100n);
    const numberSide = at("b", 100);

    // 同一时刻、类型不同：必须落到 sessionId 兜底，且两个方向必须互为相反
    expect(compareByTimelineDesc(bigintSide, numberSide)).toBeLessThan(0);
    expect(compareByTimelineDesc(numberSide, bigintSide)).toBeGreaterThan(0);

    expect(
      sortByTimelineDesc([at("late", 300), at("early", 100n), at("mid", 200)]).map(
        (s) => s.sessionId,
      ),
    ).toEqual(["late", "mid", "early"]);
  });

  it("吃得下移动端的 bigint（相减会 TypeError，所以实现只能用比较）", () => {
    const sorted = sortByTimelineDesc([at("a", 100n), at("b", 300n), at("c", 200n)]);

    expect(sorted.map((s) => s.sessionId)).toEqual(["b", "c", "a"]);
  });
});
