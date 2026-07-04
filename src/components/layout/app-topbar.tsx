/**
 * AppTopBar
 * 桌面端全局顶栏:Logo(纯标识不可点)+ 节点状态 pill + 面包屑导航 + 设置
 *
 * 导航策略:
 * - Logo 只作为品牌标识,不再承担返回主页职责
 * - 面包屑首段「主页」是唯一的回主页入口(home icon)
 * - 中间段(如「传输历史」)可点击跳上一级,末段(当前页)不可点
 */

import { Fragment, useMemo, useState } from "react";
import { Link, useLocation } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Settings,
  Minus,
  Square,
  X,
  Home,
  ArrowRightLeft,
  Inbox,
  Moon,
  Sun,
} from "lucide-react";
import { useTheme } from "next-themes";
import { Trans, useLingui } from "@lingui/react/macro";
import { cn, isMac } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { useNetworkStore, type NodeStatus } from "@/stores/network-store";
import { useTransferStore } from "@/stores/transfer-store";
import { isProjectionActive } from "@/lib/transfer-projection";
import { StartNodeSheet } from "@/components/network/start-node-sheet";
import { StopNodeSheet } from "@/components/network/stop-node-sheet";

type IconComp = React.ComponentType<{ className?: string }>;

interface CrumbSegment {
  icon: IconComp;
  label: React.ReactNode;
  /** 末段(当前页)不传 to */
  to?: "/devices" | "/transfer" | "/inbox";
}

/** 根据当前路径生成面包屑层级(主页 + 当前位置链路) */
function buildBreadcrumb(pathname: string): CrumbSegment[] {
  const home: CrumbSegment = {
    icon: Home,
    label: <Trans>主页</Trans>,
    to: "/devices",
  };

  if (pathname.startsWith("/settings")) {
    return [home, { icon: Settings, label: <Trans>设置</Trans> }];
  }
  if (pathname === "/inbox") {
    return [home, { icon: Inbox, label: <Trans>收件箱</Trans> }];
  }
  if (pathname.startsWith("/transfer")) {
    return [home, { icon: ArrowRightLeft, label: <Trans>传输活动</Trans> }];
  }
  // 主页 / 其他路径:单段当前主页(BreadcrumbPage)
  return [{ ...home, to: undefined }];
}

export function AppTopBar() {
  const { t } = useLingui();
  const status = useNetworkStore((s) => s.status);
  const location = useLocation();
  const [startOpen, setStartOpen] = useState(false);
  const [stopOpen, setStopOpen] = useState(false);

  const crumbs = useMemo(
    () => buildBreadcrumb(location.pathname),
    [location.pathname],
  );

  return (
    <>
      <header
        data-tauri-drag-region
        className={cn(
          "app-topbar relative z-20 flex h-11 shrink-0 items-center justify-between overflow-hidden border-b border-white/[0.30] bg-white/[0.18] pr-4 shadow-[0_1px_0_rgba(255,255,255,0.34),0_16px_42px_rgba(15,23,42,0.05)] backdrop-blur-xl dark:border-white/[0.07] dark:bg-slate-950/[0.08] dark:shadow-[0_1px_0_rgba(255,255,255,0.05),0_16px_42px_rgba(0,0,0,0.10)] lg:pr-5",
          // macOS 左侧给系统红绿灯按钮留出位置：pl 和 pr 分开写，
          // 避免 lg:px-5 这类同时设置左右内边距的响应式工具类
          // 在 ≥1024px 时把这里覆盖回 20px（此前 pl-20 在大屏下被 lg:px-5 顶掉，
          // 小屏下 lg: 不生效反而是对的，看起来像"只在大屏出问题"）
          isMac ? "pl-20" : "pl-4 lg:pl-5",
        )}
      >
        {/* 左:Logo(纯图标) + 状态 pill + 面包屑 */}
        <div
          data-tauri-drag-region
          className="relative z-10 flex items-center gap-2.5"
        >
          <img
            src="/app-icon.svg"
            alt="SwarmDrop"
            className="size-6 shrink-0 rounded-md"
          />

          <StatusPill
            status={status}
            onClick={() =>
              status === "running" ? setStopOpen(true) : setStartOpen(true)
            }
          />

          <Breadcrumb>
            <BreadcrumbList>
              {crumbs.map((seg, idx) => {
                const isLast = idx === crumbs.length - 1;
                const Icon = seg.icon;
                return (
                  <Fragment key={idx}>
                    {idx > 0 && <BreadcrumbSeparator />}
                    <BreadcrumbItem>
                      {isLast || !seg.to ? (
                        <BreadcrumbPage className="flex items-center gap-1 font-medium">
                          <Icon className="size-3.5" />
                          {seg.label}
                        </BreadcrumbPage>
                      ) : (
                        <BreadcrumbLink asChild>
                          <Link
                            to={seg.to}
                            className="flex items-center gap-1"
                          >
                            <Icon className="size-3.5" />
                            {seg.label}
                          </Link>
                        </BreadcrumbLink>
                      )}
                    </BreadcrumbItem>
                  </Fragment>
                );
              })}
            </BreadcrumbList>
          </Breadcrumb>
        </div>

        {/* 右:设置 + (非 Mac)窗口控制 */}
        <div
          data-tauri-drag-region
          className="relative z-10 flex items-center gap-1"
        >
          <ThemeShortcut />

          {/* ≥1280px 图标带文字标签（识别而非回忆），窄窗口回落 icon-only（title/aria 兜底） */}
          <Button
            asChild
            variant="ghost"
            className="h-8 gap-1.5 rounded-md px-2 hover:bg-foreground/[0.055] dark:hover:bg-white/[0.075]"
          >
            <Link to="/inbox" aria-label={t`收件箱`} title={t`收件箱`}>
              <Inbox className="size-4" />
              <span className="hidden text-xs font-medium xl:inline">
                <Trans>收件箱</Trans>
              </span>
            </Link>
          </Button>

          <Button
            asChild
            variant="ghost"
            className="h-8 gap-1.5 rounded-md px-2 hover:bg-foreground/[0.055] dark:hover:bg-white/[0.075]"
          >
            <Link
              to="/transfer"
              aria-label={t`传输活动`}
              title={t`传输活动`}
              className="relative"
            >
              <ArrowRightLeft className="size-4" />
              <span className="hidden text-xs font-medium xl:inline">
                <Trans>传输</Trans>
              </span>
              <ActiveTransferBadge />
            </Link>
          </Button>

          <Button
            asChild
            variant="ghost"
            className="h-8 gap-1.5 rounded-md px-2 hover:bg-foreground/[0.055] dark:hover:bg-white/[0.075]"
          >
            <Link to="/settings" aria-label={t`设置`} title={t`设置`}>
              <Settings className="size-4" />
              <span className="hidden text-xs font-medium xl:inline">
                <Trans>设置</Trans>
              </span>
            </Link>
          </Button>

          {!isMac && <WindowControls />}
        </div>
      </header>

      <StartNodeSheet open={startOpen} onOpenChange={setStartOpen} />
      <StopNodeSheet open={stopOpen} onOpenChange={setStopOpen} />
    </>
  );
}

/**
 * 活跃传输计数徽章:有进行中的传输时挂在顶栏传输图标右上角,
 * 让用户离开传输页也能看到"有东西正在传"。
 */
function ActiveTransferBadge() {
  // 只订阅 projections 引用：高频 progress 事件不改它，计数只在会话状态真变时重算
  const projections = useTransferStore((s) => s.projections);
  const activeCount = useMemo(
    () => Object.values(projections).filter(isProjectionActive).length,
    [projections],
  );
  if (activeCount === 0) return null;
  return (
    <span className="absolute right-0.5 top-0.5 flex h-[15px] min-w-[15px] items-center justify-center rounded-full bg-primary px-1 font-mono text-[9px] font-semibold leading-none text-primary-foreground">
      {activeCount}
    </span>
  );
}

/** 顶栏快捷主题切换:完整的跟随系统选项仍保留在设置页 */
function ThemeShortcut() {
  const { t } = useLingui();
  const { resolvedTheme, setTheme } = useTheme();
  const isDark = resolvedTheme === "dark";
  const label = isDark ? t`切换到浅色主题` : t`切换到深色主题`;
  const Icon = isDark ? Sun : Moon;

  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label={label}
      title={label}
      onClick={() => setTheme(isDark ? "light" : "dark")}
      className="size-8 rounded-md hover:bg-foreground/[0.055] dark:hover:bg-white/[0.075]"
    >
      <Icon className="size-4" />
    </Button>
  );
}

/** Windows / Linux 自画窗口控制按钮(最小化 / 最大化 / 关闭) */
function WindowControls() {
  const { t } = useLingui();
  const appWindow = getCurrentWindow();

  return (
    <>
      <div className="ml-1 h-5 w-px bg-foreground/10 dark:bg-white/10" />
      <button
        type="button"
        onClick={() => appWindow.minimize()}
        aria-label={t`最小化`}
        className="flex h-8 w-10 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.055] dark:hover:bg-white/[0.075]"
      >
        <Minus className="size-4" />
      </button>
      <button
        type="button"
        onClick={() => appWindow.toggleMaximize()}
        aria-label={t`最大化`}
        className="flex h-8 w-10 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-foreground/[0.055] dark:hover:bg-white/[0.075]"
      >
        <Square className="size-3.5" />
      </button>
      <button
        type="button"
        onClick={() => appWindow.close()}
        aria-label={t`关闭`}
        className="flex h-8 w-10 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
      >
        <X className="size-4" />
      </button>
    </>
  );
}

interface StatusPillConfig {
  bg: string;
  text: string;
  dot: string;
  label: React.ReactNode;
}

const pillStyles: Record<NodeStatus, StatusPillConfig> = {
  running: {
    bg: "bg-green-100 dark:bg-emerald-500/15",
    text: "text-green-700 dark:text-emerald-300",
    dot: "bg-green-600",
    label: <Trans>在线 · 可接收</Trans>,
  },
  starting: {
    bg: "bg-amber-100 dark:bg-amber-900/30",
    text: "text-amber-700 dark:text-amber-300",
    dot: "bg-amber-500 animate-pulse",
    label: <Trans>启动中</Trans>,
  },
  stopped: {
    bg: "bg-zinc-100 dark:bg-secondary",
    text: "text-zinc-600 dark:text-muted-foreground",
    dot: "bg-zinc-400",
    label: <Trans>未启动</Trans>,
  },
  error: {
    bg: "bg-red-100 dark:bg-red-900/30",
    text: "text-red-700 dark:text-red-300",
    dot: "bg-red-600",
    label: <Trans>节点错误</Trans>,
  },
};

function StatusPill({
  status,
  onClick,
}: {
  status: NodeStatus;
  onClick: () => void;
}) {
  const config = pillStyles[status];

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-full px-2.5 py-1 transition-opacity hover:opacity-80",
        config.bg,
      )}
    >
      <span className={cn("size-1.5 rounded-full", config.dot)} />
      <span className={cn("text-[11px] font-medium", config.text)}>
        {config.label}
      </span>
    </button>
  );
}
