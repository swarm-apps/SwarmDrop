"use client";

// 一条会话此刻**还能拿出来给人看**的两个实时数：剩余时间与速率。
//
// 本文件只做两件事：**订阅**那一帧的到达时刻，**读**当前时刻。判据本身
// （「这一帧还算不算数」「哪些数有保质期」）整条在 `@swarmdrop/shared-view` 的
// `usableRates` 里——三端各有一份渲染代码，判据分家就会变成一端 6 秒、一端 10 秒、
// 一端忘了做，或者更糟：同一端把速率与剩余时间分开判，于是同一行里一个诚实一个撒谎。
//
// ## 「停滞那一刻会重算」靠的不是这个 hook
//
// 陈旧发生时没有任何新事件，也就没有重渲染。真正戳一下的是 store 里那个保鲜期定时器
// （`armStaleTimer`）：到点抹掉 `progressAt[sessionId]`，下面这个 selector 的返回值随之
// 从一个数变成 `undefined`，订阅者才被叫醒。少了它，判据写得再对也永远不会被重新求值。

import { usableRates, type UsableRates } from "@swarmdrop/shared-view";
import { useWebNode } from "./store";
import type { TransferProgressEvent } from "./view-types";

/**
 * @param sessionId 会话 id——到达时刻按它索引。
 * @param live `transferSample` 给出的在途帧（终态会话为 `undefined`，那时两个数都为 `null`）。
 */
export function useUsableRates(
  sessionId: string,
  live: TransferProgressEvent | undefined,
): UsableRates {
  // selector 返回 `number | undefined`，是原始值——不违反「selector 里不派生新对象」那条
  // （`pnpm check:zustand-access` 的规则 B 覆盖本目录）。
  const receivedAt = useWebNode((s) => s.progressAt[sessionId]);
  return usableRates(live, receivedAt, Date.now());
}
