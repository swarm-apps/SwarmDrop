/**
 * Inbox Route Config
 * 收件箱路由配置 —— 用 searchParams 表达 master-detail 选中态与可见过滤。
 */

import { createFileRoute } from "@tanstack/react-router";

interface InboxSearch {
  item?: string;
  q?: string;
  archived?: boolean;
}

export const Route = createFileRoute("/_app/inbox/")({
  validateSearch: (search: Record<string, unknown>): InboxSearch => {
    const item = search.item;
    const q = search.q;
    const archived = search.archived;
    return {
      ...(typeof item === "string" && item !== "" ? { item } : {}),
      ...(typeof q === "string" && q.trim() !== "" ? { q } : {}),
      ...(archived === true || archived === "true" ? { archived: true } : {}),
    };
  },
});
