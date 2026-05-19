/**
 * Onboarding Layout
 * 首启引导布局 —— macOS 留 32px drag region 让窗口可拖；其他平台无 chrome。
 */

import { createFileRoute, Outlet } from "@tanstack/react-router";
import { isMac } from "@/lib/utils";

export const Route = createFileRoute("/_onboarding")({
  component: OnboardingLayout,
});

function OnboardingLayout() {
  return (
    <div className="flex h-svh flex-col bg-background">
      {isMac && (
        <div data-tauri-drag-region className="h-8 shrink-0 bg-background" />
      )}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  );
}
