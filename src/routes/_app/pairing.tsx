/**
 * Pairing Layout Route
 * 配对路由组布局
 *
 * 作用：
 * - 作为所有配对相关子路由的父布局（pairing/index, pairing/input, pairing/generate）
 * - 路由守卫：节点未运行时拦截进入，redirect 回首页并 toast 提示
 * - 保留扩展点：将来可在此添加配对流程的全局状态或 UI（如进度条、步骤指示器等）
 *
 * 注意：不要删除此文件，否则子路由的 URL 结构会改变
 */

import { createFileRoute, Outlet, redirect } from "@tanstack/react-router";
import { toast } from "sonner";
import { t } from "@lingui/core/macro";
import { useNetworkStore } from "@/stores/network-store";

export const Route = createFileRoute("/_app/pairing")({
  beforeLoad: ({ location }) => {
    const { status } = useNetworkStore.getState();
    if (status !== "running") {
      toast.error(t`请先启动节点`, {
        description: t`配对功能需要 P2P 节点处于运行状态`,
      });
      throw redirect({ to: "/devices" });
    }
    // 精确访问 /pairing 时默认走「生成配对码」(原 mobile index 已下线)
    if (location.pathname === "/pairing" || location.pathname === "/pairing/") {
      throw redirect({ to: "/pairing/generate" });
    }
  },
  component: () => <Outlet />,
});
