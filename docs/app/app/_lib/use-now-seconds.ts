"use client";

import { useSyncExternalStore } from "react";

/**
 * 当前 Unix 秒，由**一个进程内共享的节拍**推进。
 *
 * **tick 的是「现在」而不是「还剩多少」**：剩余量随之变成纯推导，于是同一个组件里的
 * 倒计时、过期判定、列表里每一行的剩余期都读同一个 `now`，不会出现「码上说还剩 1 分钟、
 * 列表里那行还冻在 23 小时」这种自相矛盾。
 *
 * 节拍**共享**而不是每个调用点各建一个 `setInterval`：相对时间进列表之后（传输会话行、
 * 收件箱条目行）同屏可以有几十个调用点，各自计时既是几十个定时器、相位还各不相同——
 * 相邻两行会在不同的时刻翻页，一个已经写着「1 分钟前」，另一个还停在「刚刚」。
 * 上面那句「都读同一个 now」要成立，节拍就必须只有一个。
 *
 * 30 秒：邀请 TTL 是 24 小时、相对时间最细一档是分钟，秒级刷新既无意义又白烧渲染；
 * 30 秒足够让「还有 1 分钟」这种末段变化被看见。
 */
const TICK_MS = 30_000;

const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | undefined;
let now = Math.floor(Date.now() / 1000);

/**
 * SSR / 预渲染快照。**恒为模块加载那一刻的值**——`useSyncExternalStore` 要求服务端快照
 * 在一次渲染里稳定。当前没有调用点会进预渲染产物（相对时间与邀请倒计时读的都是运行时
 * 数据，构建期一条都没有），所以它实际只是个占位。
 */
const serverNow = now;

function subscribe(onStoreChange: () => void): () => void {
  listeners.add(onStoreChange);
  if (timer === undefined) {
    // **先拨表再开表。** 停表期间 `now` 一直冻在最后一次 tick 上，而「没人看」可以持续很久
    // ——用户在设备页待了十分钟再进传输页，第一个订阅者拿到的就是十分钟前的「现在」，
    // 一屏的相对时间集体少算十分钟，还要等满一个节拍才会自己纠正。
    // （React 会在 subscribe 之后重新读一次快照，所以这里改完不用另行通知。）
    now = Math.floor(Date.now() / 1000);
    timer = setInterval(() => {
      now = Math.floor(Date.now() / 1000);
      for (const listener of listeners) listener();
    }, TICK_MS);
  }

  return () => {
    listeners.delete(onStoreChange);
    // 最后一个订阅者走了就停表——没人看的时候不该还留着一个每 30 秒醒一次的定时器。
    if (listeners.size === 0 && timer !== undefined) {
      clearInterval(timer);
      timer = undefined;
    }
  };
}

export function useNowSeconds(): number {
  return useSyncExternalStore(
    subscribe,
    () => now,
    () => serverNow,
  );
}
