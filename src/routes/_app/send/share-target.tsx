/**
 * Share Target Route Config
 * 右键/外部打开快捷发送页路由配置 —— session 表达发送后就地进度会话。
 */

import { createFileRoute } from "@tanstack/react-router";

interface ShareTargetSearch {
  session?: string;
}

export function validateShareTargetSearch(
  search: Record<string, unknown>,
): ShareTargetSearch {
  const session = search.session;
  return typeof session === "string" && session !== "" ? { session } : {};
}

export const Route = createFileRoute("/_app/send/share-target")({
  validateSearch: validateShareTargetSearch,
});
