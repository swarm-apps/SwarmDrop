import { describe, expect, it } from "vitest";

import type { TransferProjection } from "../_lib/view-types";
import { matchesSessionFilter, type SessionFilter } from "./transfer-labels";

const session = (
  sessionId: string,
  phase: TransferProjection["phase"],
  recoverable = false,
) => ({ sessionId, phase, recoverable }) as unknown as TransferProjection;

const sessions = [
  session("live", "active"),
  session("paused", "suspended", true),
  session("dead", "suspended"), // 不可恢复的中断
  session("done", "terminal"),
];

const hitting = (filter: SessionFilter) =>
  sessions.filter((s) => matchesSessionFilter(s, filter)).map((s) => s.sessionId);

describe("matchesSessionFilter", () => {
  it("全部档不筛任何东西", () => {
    expect(hitting("all")).toEqual(["live", "paused", "dead", "done"]);
  });

  it("进行中 = 非终态（本端把 suspended 也算进来，与桌面判据尚不同义）", () => {
    expect(hitting("active")).toEqual(["live", "paused", "dead"]);
  });

  it("可恢复只认 suspended + recoverable", () => {
    expect(hitting("recoverable")).toEqual(["paused"]);
  });

  it("已结束是进行中的补集——两档合起来必须盖满全集，不重不漏", () => {
    expect(hitting("ended")).toEqual(["done"]);
    expect([...hitting("active"), ...hitting("ended")].sort()).toEqual(
      sessions.map((s) => s.sessionId).sort(),
    );
  });
});
