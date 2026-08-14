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
  ArrowLeftRight,
  Inbox,
  Link2,
  Moon,
  Send,
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
import { useActiveTransferCount } from "@/hooks/use-active-transfer-count";
import { useNodeHealth } from "@/hooks/use-node-health";
import { resolveNodePresentation, TONE_BADGE, TONE_DOT } from "@/lib/node-status";
import { NodeStatusSheet } from "@/components/network/node-status-sheet";

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
    return [home, { icon: ArrowLeftRight, label: <Trans>传输活动</Trans> }];
  }
  if (pathname.startsWith("/send")) {
    return [home, { icon: Send, label: <Trans>发送内容</Trans> }];
  }
  if (pathname.startsWith("/pairing")) {
    return [home, { icon: Link2, label: <Trans>添加设备</Trans> }];
  }
  // 主页 / 其他路径:单段当前主页(BreadcrumbPage)
  return [{ ...home, to: undefined }];
}

export function AppTopBar() {
  const { t } = useLingui();
  const location = useLocation();
  const [nodeSheetOpen, setNodeSheetOpen] = useState(false);

  const crumbs = useMemo(
    () => buildBreadcrumb(location.pathname),
    [location.pathname],
  );

  return (
    <>
      <header
        data-tauri-drag-region
        data-testid="app-topbar"
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

          <StatusPill onClick={() => setNodeSheetOpen(true)} />

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
            <Link
              to="/inbox"
              aria-label={t`收件箱`}
              title={t`收件箱`}
              data-testid="topbar-inbox-link"
            >
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
              data-testid="topbar-transfer-link"
              className="relative"
            >
              <ArrowLeftRight className="size-4" />
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
            <Link
              to="/settings"
              aria-label={t`设置`}
              title={t`设置`}
              data-testid="topbar-settings-link"
            >
              <Settings className="size-4" />
              <span className="hidden text-xs font-medium xl:inline">
                <Trans>设置</Trans>
              </span>
            </Link>
          </Button>

          {!isMac && <WindowControls />}
        </div>
      </header>

      <NodeStatusSheet open={nodeSheetOpen} onOpenChange={setNodeSheetOpen} />
    </>
  );
}

/**
 * 活跃传输计数徽章:有进行中的传输时挂在顶栏传输图标右上角,
 * 让用户离开传输页也能看到"有东西正在传"。
 */
function ActiveTransferBadge() {
  const activeCount = useActiveTransferCount();
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

/** Windows / Linux 自画窗口控制按钮(最小化 / 最大化 / 关闭)。
 *  全屏路由（发送 / 配对）隐藏 AppTopBar 时，需在页面自己的顶栏复用它。 */
export function WindowControls() {
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

/**
 * 节点状态 pill —— 结论层的常驻位。
 *
 * 状态词与色档来自 `summarizeNodeHealth`（三端同一份判据），**不再只看生命周期**：
 * 此前节点在跑就是绿的「在线 · 可接收」，哪怕全部中继都连不上、跨网设备根本找不到你。
 *
 * 配色仍是与移动端 `status-pill.tsx` 同一套语汇：底 `/15`、文字走 `-ink` 变体、
 * 圆点用状态色本体（State Ink Rule）。
 */
function StatusPill({ onClick }: { onClick: () => void }) {
  const { t } = useLingui();
  const { summary, lifecycle } = useNodeHealth();
  const presentation = resolveNodePresentation(lifecycle, summary);

  return (
    <button
      type="button"
      onClick={onClick}
      data-testid="network-status-pill"
      data-node-status={lifecycle}
      data-node-health={summary.level}
      aria-label={t(presentation.sentence)}
      title={t`点开查看节点状态与诊断信息`}
      className={cn(
        "flex items-center gap-1.5 rounded-full px-2.5 py-1 transition-opacity hover:opacity-80",
        TONE_BADGE[presentation.tone],
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          TONE_DOT[presentation.tone],
          lifecycle === "starting" && "animate-pulse",
        )}
      />
      <span className="text-[11px] font-medium">{t(presentation.word)}</span>
    </button>
  );
}
