/**
 * Transfer Detail Redirect
 * 旧独立详情页深链（通知点击 / 历史外链）→ 活动中心 master-detail 选中态。
 */

import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/transfer/$sessionId")({
  beforeLoad: ({ params }) => {
    throw redirect({
      to: "/transfer",
      search: { session: params.sessionId },
      replace: true,
    });
  },
});
