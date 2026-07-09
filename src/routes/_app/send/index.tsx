/**
 * Send Route Config
 * 发送页面路由配置 — 校验 searchParams
 */

import { createFileRoute } from "@tanstack/react-router";

interface SendSearch {
  peerId: string;
  session?: string;
}

export function validateSendSearch(search: Record<string, unknown>): SendSearch {
  const peerId = search.peerId;
  const session = search.session;
  return {
    peerId: typeof peerId === "string" ? peerId : "",
    ...(typeof session === "string" && session !== "" ? { session } : {}),
  };
}

export const Route = createFileRoute("/_app/send/")({
  validateSearch: validateSendSearch,
});
