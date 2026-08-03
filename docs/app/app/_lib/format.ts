// Web 应用区的展示派生。
//
// **格式化已收口到 `@swarmdrop/shared-view`**（三端同一份取整与单位规则），组件**直接从那里导入**——
// 本模块不做转发。转发会让「格式化从哪来」有两个答案，而两个答案迟早各长各的。
// 这里只留一类东西：依赖 Web 侧 `TransferProjection` 类型的投影派生。
//
// **占位文案不在这里**。共享的 `formatTransferRate` 在算不出速率时返回 `null`，
// `formatDuration` 只收确定值——「等待数据」「未知」这类占位是要翻译的 UI 文案，
// 而翻译宏只在组件里展开，放进本模块就成了永远的中文。调用点自己给。

import type { TransferProjection } from "./view-types";

/**
 * 会话「结束时刻」：终态会话有 `finishedAt`，非终态回退到最后更新。
 * 收件箱排序与活动视图的耗时计算共用同一个定义。
 */
export function sessionEndedAt(projection: TransferProjection): number {
  return projection.finishedAt ?? projection.updatedAt;
}

/** 按最后更新时间倒序（不改原数组）。 */
export function sortByUpdatedDesc(items: TransferProjection[]): TransferProjection[] {
  return [...items].sort((a, b) => b.updatedAt - a.updatedAt);
}

/**
 * 会话是否仍在进行中。传输页的「N 个进行中」与导航徽标的计数共用同一个判定——
 * 各写一份的话，将来加一个非 terminal 的新 phase，两处数字就会对不上。
 */
export function isActiveSession(projection: TransferProjection): boolean {
  return projection.phase !== "terminal";
}

/**
 * 会话记录是否可删除：仅已结束与已中断（suspended）。
 *
 * 与内核域层守卫 `swarmdrop_transfer::store::is_deletable` 同一判据——按钮可见性只是第一道，
 * 绕过它直调 `delete_transfer_session()` 会被 `TransferManager::delete_session` 拒掉。
 * **不要拿 `isActiveSession` 取反**：那条判的是「还没结束」（suspended 算在内，导航徽标要数它），
 * 而 suspended 是可以删的——它没有活 actor，代价只是断点信息一并消失。
 */
export function isDeletableSession(projection: TransferProjection): boolean {
  return projection.phase === "terminal" || projection.phase === "suspended";
}
