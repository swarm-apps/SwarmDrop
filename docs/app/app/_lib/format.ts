// Web 应用区的展示派生。
//
// **格式化已收口到 `@swarmdrop/shared-view`**（三端同一份取整与单位规则），组件**直接从那里导入**——
// 本模块不做转发。转发会让「格式化从哪来」有两个答案，而两个答案迟早各长各的。
// 这里只留一类东西：依赖 Web 侧 `TransferProjection` 类型的投影派生。
//
// **占位文案不在这里**。共享的 `formatTransferRate` 在算不出速率时返回 `null`，
// `formatDuration` 只收确定值——「等待数据」「未知」这类占位是要翻译的 UI 文案，
// 而翻译宏只在组件里展开，放进本模块就成了永远的中文。调用点自己给。

import { calcPercent } from "@swarmdrop/shared-view";
import type { TransferProgressEvent, TransferProjection } from "./view-types";

/**
 * 一条会话「传到哪了」的快照。
 *
 * 它存在的唯一理由是把一条**踩过的纪律**变成机制：**终态一律以 projection 为准**。
 * `progress` 是在途采样，会话一进终态内核就不再下发，最后收到的那一帧会永远停在那儿；
 * 而 projection 在终态已回填全量字节。2026-07-28 实测：16 MiB 传完、
 * `projection.transferredBytes` 已是 16777216，采样却停在 3932160，界面显示
 * 「已完成 · 23%」。
 *
 * 此前这段取舍在三个渲染点各写了一遍（活动列表行 / 会话详情 / 发送页的「已发出」卡片），
 * 每一遍都靠一段注释提醒下一个人。第四个渲染点不会记得。
 */
export function transferSample(
  projection: TransferProjection,
  progress?: TransferProgressEvent,
): { live: TransferProgressEvent | undefined; done: number; total: number; percent: number } {
  const live = projection.phase === "terminal" ? undefined : progress;
  const done = live?.transferredBytes ?? projection.transferredBytes;
  const total = live?.totalBytes ?? projection.totalSize;
  return { live, done, total, percent: calcPercent(done, total) };
}

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
 * 会话是否可暂停：**仅 `active`**。
 *
 * 与内核的转换守卫同一判据——`reduce_user` 里写的是
 * `UserCommand::Pause if state.is_active()`，而 `is_active()` 就是 `phase == Active`。
 * 按钮可见性只是第一道：绕过它直调 `pause_send()`，协调器会把它判成无效转换。
 *
 * **不要拿 `isActiveSession` 来判这件事**（名字很像，含义不同）：那条判的是「还没结束」，
 * 等待对方接受、已中断都算在内——而那些阶段根本没有在跑的 actor，暂停无从谈起。
 */
export function isPausableSession(projection: TransferProjection): boolean {
  return projection.phase === "active";
}

/**
 * 会话是否可续传：`suspended` 且内核判定断点仍然可用。
 *
 * `recoverable` 是内核给的事实（源文件还在、checkpoint 没失效），不是 UI 能推的——
 * 所以判据只做「读它」这一件事，收在这里是为了让传输页的筛选与详情侧的按钮可见性
 * 用同一句话。两处各写一遍的话，筛出来的「可恢复」里会混进没有续传按钮的条目。
 */
export function isRecoverableSession(projection: TransferProjection): boolean {
  return projection.phase === "suspended" && projection.recoverable;
}

/**
 * 这条会话**刷新页面后会不会消失**：非终态的发送会话都会。
 *
 * 判据与内核落库规则同源——`crates/web/src/store.rs` 的 `worth_persisting` 在发送方向上
 * 只对 `Terminal` 返回 true，其余（`Offered` / `WaitingAccept` / `Active` / `Suspended`）
 * 一律不落 IndexedDB，所以刷新后连记录都不在了。
 *
 * 根因是浏览器的物理约束而非实现取舍：发送侧的文件内容来自用户选中的 `File` 对象，页面一
 * 刷新 JS 上下文就销毁，浏览器不允许在用户未重新选择的前提下再读同一个文件。落库反而更糟
 * ——用户会看到一个点了必失败的「续传」按钮。
 *
 * **接收方向不在此列**：半成品与 checkpoint 都在 OPFS 里，刷新后续得上。这个不对称是本判据
 * 存在的全部意义，别用 `isActiveSession` 之类的方向无关判据代替它。
 *
 * 接收方向的待决 offer（`offered`）同样不落库，但**刻意不算在这里**：它还没有任何进度可丢，
 * 重来的成本只是让对端再发一次，为它拦截刷新是纯打扰。本判据的语义是「有进度会丢」。
 */
export function isLostOnReload(projection: TransferProjection): boolean {
  return projection.direction === "send" && projection.phase !== "terminal";
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
