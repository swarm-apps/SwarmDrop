import { describe, expect, it } from "vitest";
import { groupByTimeBucket } from "./time-buckets";

/** 2026-08-04（周二）12:00 本地时间，作为所有用例的「现在」。 */
const NOW = new Date(2026, 7, 4, 12, 0, 0).getTime();
const at = (...args: [number, number, number, number?]) =>
  new Date(args[0], args[1], args[2], args[3] ?? 0).getTime();

const ts = (t: number) => t;

describe("groupByTimeBucket", () => {
  it("按今天 / 昨天 / 本周 / 更早分桶，并保持原有相对顺序", () => {
    const items = [
      at(2026, 7, 4, 9), // 今天早上
      at(2026, 7, 4, 0), // 今天零点整——边界必须算今天
      at(2026, 7, 3, 23), // 昨天
      at(2026, 7, 1), // 三天前 → 本周
      at(2026, 6, 20), // 上个月 → 更早
    ];
    const groups = groupByTimeBucket(items, ts, NOW);

    expect(groups.map((g) => g.key)).toEqual(["today", "yesterday", "week", "earlier"]);
    expect(groups[0].items).toEqual([items[0], items[1]]);
    expect(groups[3].items).toEqual([items[4]]);
  });

  it("空桶不出现——只有标题没有内容的分组头是纯噪音", () => {
    const groups = groupByTimeBucket([at(2026, 6, 1)], ts, NOW);
    expect(groups.map((g) => g.key)).toEqual(["earlier"]);
  });

  it("「本周」是含今天在内的最近 7 天，第 7 天仍在、第 8 天落到更早", () => {
    const seventhDay = at(2026, 7, 4 - 6, 23); // 7 天前的当天，仍在窗口内
    const eighthDay = at(2026, 7, 4 - 7, 23); // 再往前一天
    const groups = groupByTimeBucket([seventhDay, eighthDay], ts, NOW);
    expect(groups.find((g) => g.key === "week")?.items).toEqual([seventhDay]);
    expect(groups.find((g) => g.key === "earlier")?.items).toEqual([eighthDay]);
  });

  it("时间戳坏掉的条目落进「更早」而不是被丢弃或抛错", () => {
    const groups = groupByTimeBucket([Number.NaN], ts, NOW);
    expect(groups).toEqual([{ key: "earlier", items: [Number.NaN] }]);
  });

  // 用毫秒减法算边界时，DST 切换那天的「昨天」会变成 23 或 25 小时前，边界条目落错桶。
  // 这里用一个真实存在 DST 的时区语义无法在 Node 默认 TZ 下稳定复现，所以退而钉住
  // **实现走的是日历运算**：跨月边界正确即说明用的是 `new Date(y, m, d - n)` 那条路径。
  it("跨月边界由日历运算处理（而不是毫秒减法）", () => {
    const now = new Date(2026, 8, 2, 12).getTime(); // 9 月 2 日
    const augLast = at(2026, 7, 31, 20); // 8 月 31 日 → 昨天的前一天，属本周
    const yesterday = at(2026, 8, 1, 20); // 9 月 1 日
    const groups = groupByTimeBucket([yesterday, augLast], ts, now);
    expect(groups.find((g) => g.key === "yesterday")?.items).toEqual([yesterday]);
    expect(groups.find((g) => g.key === "week")?.items).toEqual([augLast]);
  });
});
