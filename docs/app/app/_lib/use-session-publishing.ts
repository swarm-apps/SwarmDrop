"use client";

// 一条会话此刻**正在保存哪个文件**（接收的「暂存 → 发布」第二段），没有则 `null`。

import { useWebNode } from "./store";
import type { FilePublishEvent, TransferProjection } from "./view-types";

/**
 * 按 projection 订阅「正在保存的文件」。对齐桌面 `src/stores/transfer-store.ts` 的同名
 * selector，另把**阶段收窄折进来**——本端四处渲染点此前各自在调用后补一行
 * `active ? publishingFile : undefined`，判据抄了四遍。
 *
 * ## 为什么要收窄，而不是直读那个域
 *
 * 发布是 active 会话**内部**的一段。任何遗留一条 publishing 条目的路径（事件乱序、
 * 揭示定时器与终态赛跑）都不该让一条已经结束的行永久写着「正在保存 x.zip」并顶着一条
 * 灰进度条。store 侧本就有清理（`transferProjection` 转非 active 时摘条目 + 撤定时器），
 * 这里是同一条判据的第二道。
 *
 * 收窄**做在 selector 里面**是刻意的：非 active 的会话恒返回 `null`，于是任何一条别的
 * 会话开始发布都不会把它叫醒。传输列表里绝大多数是终态行，这条省掉的正是那一片。
 *
 * 返回的是 store 里那份对象的原引用（或 `null`），不派生新对象——`pnpm check:zustand-access`
 * 的规则 B 覆盖本目录。
 */
export function useSessionPublishing(
  projection: TransferProjection,
): FilePublishEvent | null {
  return useWebNode((s) =>
    projection.phase === "active"
      ? (s.publishingBySession[projection.sessionId] ?? null)
      : null,
  );
}
