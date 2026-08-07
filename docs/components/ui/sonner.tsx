"use client";

// 与桌面 `src/components/ui/sonner.tsx` 同一份封装（图标集、位置、token 映射逐字一致），
// 只多一句 "use client"——docs 是 App Router，默认 server component。
//
// Web 应用区此前**没有任何 toast**：复制邀请、撤销、归档、清空历史、下载，做完全部静默，
// 而桌面同样的动作有 76 处 `toast.*`。DESIGN.md 跨端清单里「Destructive actions …
// **and can report failure**」在 Web 上只兑现了前半句。

import {
  CircleCheckIcon,
  InfoIcon,
  Loader2Icon,
  OctagonXIcon,
  TriangleAlertIcon,
} from "lucide-react";
import { useTheme } from "next-themes";
import { Toaster as Sonner, type ToasterProps } from "sonner";

const Toaster = ({ ...props }: ToasterProps) => {
  const { theme = "system" } = useTheme();

  return (
    <Sonner
      theme={theme as ToasterProps["theme"]}
      position="top-center"
      closeButton
      className="toaster group"
      icons={{
        success: <CircleCheckIcon className="size-4" />,
        info: <InfoIcon className="size-4" />,
        warning: <TriangleAlertIcon className="size-4" />,
        error: <OctagonXIcon className="size-4" />,
        loading: <Loader2Icon className="size-4 animate-spin" />,
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      {...props}
    />
  );
};

export { Toaster };
