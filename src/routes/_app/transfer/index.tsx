/**
 * Transfer Route Config
 * 活动中心路由配置 —— 校验 searchParams（session = 当前选中会话，master-detail 选中态）
 */

import { createFileRoute } from "@tanstack/react-router";

interface TransferSearch {
  session?: string;
}

export const Route = createFileRoute("/_app/transfer/")({
  validateSearch: (search: Record<string, unknown>): TransferSearch => {
    const session = search.session;
    return typeof session === "string" && session !== ""
      ? { session }
      : {};
  },
});
