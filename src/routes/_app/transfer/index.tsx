/**
 * Transfer Route Config
 * 活动中心路由配置 —— 校验 searchParams：
 * - session = 当前选中会话（master-detail 选中态）
 * - filter = 当前列表过滤器
 */

import { createFileRoute } from "@tanstack/react-router";

export type TransferFilterKey = "all" | "active" | "recoverable" | "ended";

const TRANSFER_FILTERS = new Set<TransferFilterKey>([
  "all",
  "active",
  "recoverable",
  "ended",
]);

interface TransferSearch {
  session?: string;
  filter?: TransferFilterKey;
}

export function validateTransferSearch(
  search: Record<string, unknown>,
): TransferSearch {
  const session = search.session;
  const filter = search.filter;
  return {
    ...(typeof session === "string" && session !== "" ? { session } : {}),
    ...(typeof filter === "string" &&
    TRANSFER_FILTERS.has(filter as TransferFilterKey) &&
    filter !== "all"
      ? { filter: filter as TransferFilterKey }
      : {}),
  };
}

export const Route = createFileRoute("/_app/transfer/")({
  validateSearch: validateTransferSearch,
});
