/**
 * Index Route —— 直接重定向到设备页。
 *
 * 旧版本会根据 auth-store 状态判断"未设置/未解锁"，分别跳 /welcome 或 /unlock。
 * 现在 setup/unlock 流程已废弃（设备身份由 host keychain 自动初始化），统一直进 /devices。
 */

import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/")({
  beforeLoad: () => {
    throw redirect({ to: "/devices" });
  },
});
