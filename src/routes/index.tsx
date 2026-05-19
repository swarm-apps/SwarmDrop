/**
 * Index Route —— 入口分流。
 *
 * 首次启动（device_name 未设置）跳 onboarding 让用户起名；之后直进 /devices。
 * device_name 由 main.tsx 在路由 mount 前 `syncDeviceNameFromBackend()` 同步到
 * preferences-store，所以这里读 store 缓存即可。
 */

import { createFileRoute, redirect } from "@tanstack/react-router";
import { usePreferencesStore } from "@/stores/preferences-store";

export const Route = createFileRoute("/")({
  beforeLoad: () => {
    const deviceName = usePreferencesStore.getState().deviceName.trim();
    if (deviceName === "") {
      throw redirect({ to: "/device-name" });
    }
    throw redirect({ to: "/devices" });
  },
});
