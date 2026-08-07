/**
 * Onboarding Layout
 * 首启引导布局 —— macOS 留 32px drag region 让窗口可拖；其他平台无 chrome。
 */

import { createFileRoute, Outlet } from "@tanstack/react-router";
import {
  AppAmbientBackground,
  AppAmbientLightOverlay,
} from "@/components/layout/app-ambient-background";
import { WindowControls } from "@/components/layout/app-topbar";
import { isMac } from "@/lib/utils";

export const Route = createFileRoute("/_onboarding")({
  component: OnboardingLayout,
});

function OnboardingLayout() {
  return (
    <div className="app-shell flex h-svh flex-col">
      <AppAmbientBackground />
      <header
        data-tauri-drag-region
        className="relative z-20 flex h-11 shrink-0 items-center justify-between border-b border-white/[0.30] bg-white/[0.18] pr-4 shadow-[0_1px_0_rgba(255,255,255,0.34),0_16px_42px_rgba(15,23,42,0.05)] backdrop-blur-xl dark:border-white/[0.07] dark:bg-slate-950/[0.08] dark:shadow-[0_1px_0_rgba(255,255,255,0.05),0_16px_42px_rgba(0,0,0,0.10)] lg:pr-5"
      >
        <div
          data-tauri-drag-region
          className={isMac ? "flex items-center gap-2 pl-20" : "flex items-center gap-2 pl-4 lg:pl-5"}
        >
          <img
            src="/app-icon.svg"
            alt=""
            className="size-5 rounded-[5px]"
          />
          <span className="text-sm font-medium text-foreground">SwarmDrop</span>
        </div>
        {!isMac && (
          <div data-tauri-drag-region className="flex items-center">
            <WindowControls />
          </div>
        )}
      </header>
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
      <AppAmbientLightOverlay />
    </div>
  );
}
