/**
 * AboutSection
 * 设置页「关于」区域 — 应用信息 + 更新状态展示（接 registry-web / SwarmHive 更新引擎）
 */

import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";
import type { Progress as UpdateProgress, UpdateStatus } from "@swarm-hive/sdk";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Download,
  ExternalLink,
  Github,
  Globe2,
  Info,
  Loader2,
  RefreshCw,
  Sparkles,
} from "lucide-react";
import { useEffect, useState, type ComponentType, type ReactNode } from "react";
import { MarkdownContent } from "@/components/ui/markdown-content";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { cn } from "@/lib/utils";

/** 格式化字节数为人类可读 */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function AboutSection() {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <Info className="size-4 text-blue-600 dark:text-blue-300" />
        <h2 className="text-sm font-semibold text-foreground">
          <Trans>关于</Trans>
        </h2>
      </div>
      <AboutPanel />
    </section>
  );
}

export function AboutPanel({
  className,
  variant = "card",
}: {
  className?: string;
  variant?: "card" | "hero";
}) {
  // 经 registry-web 的 useUpdate() 订阅 SwarmHive 更新引擎（与 __root 的 <UpdateProvider>
  // 同一个 engine）。check(true) 手动检查绕过节流；download() 触发下载，ready 后由
  // __root 常驻的 Prompt/Force 弹窗自动安装+重启。
  const { status, release, progress, check, download } = useUpdate();
  const latestVersion = release?.version ?? null;
  const releaseNotes = release?.notes ?? null;

  // 独立获取版本号，不依赖更新检查
  const [appVersion, setAppVersion] = useState<string | null>(null);
  useEffect(() => {
    getVersion().then(setAppVersion);
  }, []);

  const currentVersion = appVersion;
  const isHero = variant === "hero";

  return (
    <div
      className={cn(
        isHero
          ? "settings-about-panel-hero"
          : "glass-card overflow-hidden rounded-lg",
        className
      )}
    >
      {/* App Info Row - 桌面端 space-between，支持自动换行 */}
      <div
        className={cn(
          "flex flex-col",
          isHero
            ? "gap-3 px-1 py-2 min-[560px]:flex-row min-[560px]:items-center min-[560px]:justify-between"
            : "gap-4 p-4 min-[480px]:flex-row min-[480px]:items-center min-[480px]:justify-between"
        )}
      >
        {/* 应用信息 */}
        <div className="flex items-center gap-3">
          {!isHero ? (
            <img
              src="/app-icon.svg"
              alt="SwarmDrop"
              className="size-10 rounded-lg"
            />
          ) : null}
          <div className="flex flex-col gap-0.5">
            <span className="text-[15px] font-semibold text-foreground">
              SwarmDrop
            </span>
            <span className="text-xs text-muted-foreground">
              <VersionDescription
                status={status}
                currentVersion={currentVersion}
              />
            </span>
          </div>
        </div>

        {/* 分隔线 - 仅小屏幕显示，占满容器宽度 */}
        <div
          className={cn(
            "relative block border-t border-border",
            isHero
              ? "w-full min-[560px]:hidden"
              : "left-[-1rem] w-[calc(100%+2rem)] min-[480px]:hidden"
          )}
        />

        {/* 按钮组 */}
        <div
          className={cn(
            "flex flex-wrap items-center gap-2",
            isHero
              ? "justify-start min-[560px]:justify-end"
              : "justify-around min-[480px]:justify-end"
          )}
        >
          <OfficialWebsiteButton />
          <GithubButton />
          <ReleaseNotesButton />
          <UpdateButton
            status={status}
            latestVersion={latestVersion}
            onCheck={() => void check(true)}
            onUpdate={() => void download()}
          />
        </div>
      </div>

      {/* Update Banner / Progress */}
      {(status === "available" || status === "force-required") && releaseNotes && (
        <UpdateBanner
          latestVersion={latestVersion}
          releaseNotes={releaseNotes}
        />
      )}
      {(status === "downloading" || status === "ready") && progress && (
        <DownloadProgressBanner
          latestVersion={latestVersion}
          progress={progress}
        />
      )}
    </div>
  );
}
/** 版本描述文字 */
function VersionDescription({
  status,
  currentVersion,
}: {
  status: UpdateStatus;
  currentVersion: string | null;
}) {
  const ver = currentVersion ? `v${currentVersion}` : "";
  switch (status) {
    case "checking":
      return <Trans>版本 {ver} · 检查中...</Trans>;
    case "available":
    case "force-required":
      return <Trans>版本 {ver} · 有新版本可用</Trans>;
    case "downloading":
    case "ready":
      return <Trans>版本 {ver} · 正在更新...</Trans>;
    case "up-to-date":
      return <Trans>版本 {ver} · 已是最新版本</Trans>;
    case "error":
      return <Trans>版本 {ver} · 检查失败</Trans>;
    default:
      return <Trans>版本 {ver}</Trans>;
  }
}

/** 桌面端：官方网站按钮 */
function OfficialWebsiteButton() {
  return (
    <ExternalLinkButton
      icon={Globe2}
      label={<Trans>官网</Trans>}
      url="https://swarm-apps.github.io/SwarmDrop/"
    />
  );
}

/** 桌面端：GitHub 仓库按钮 */
function GithubButton() {
  return (
    <ExternalLinkButton
      icon={Github}
      label="GitHub"
      url="https://github.com/swarm-apps/SwarmDrop"
    />
  );
}

/** 桌面端：更新日志按钮 */
function ReleaseNotesButton() {
  return (
    <ExternalLinkButton
      icon={ExternalLink}
      label={<Trans>更新日志</Trans>}
      url="https://github.com/swarm-apps/SwarmDrop/blob/main/CHANGELOG.md"
    />
  );
}

function ExternalLinkButton({
  icon: Icon,
  label,
  url,
}: {
  icon: ComponentType<{ className?: string }>;
  label: ReactNode;
  url: string;
}) {
  return (
    <button
      type="button"
      onClick={() => openUrl(url)}
      className="flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-accent"
    >
      <Icon className="size-3.5" />
      {label}
    </button>
  );
}

/** 更新操作按钮（桌面端 + 移动端统一） */
function UpdateButton({
  status,
  latestVersion,
  onCheck,
  onUpdate,
}: {
  status: UpdateStatus;
  latestVersion: string | null;
  onCheck: () => void;
  onUpdate: () => void;
}) {
  switch (status) {
    case "checking":
      return (
        <button
          type="button"
          disabled
          className="flex items-center gap-1.5 rounded-md bg-primary/50 px-3 py-1.5 text-xs font-medium text-primary-foreground"
        >
          <Loader2 className="size-3.5 animate-spin" />
          <Trans>检查中...</Trans>
        </button>
      );

    case "available":
    case "force-required":
      return (
        <button
          type="button"
          onClick={onUpdate}
          className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <Download className="size-3.5" />
          {t`更新到 v${latestVersion ?? "?"}`}
        </button>
      );

    case "downloading":
    case "ready":
      return (
        <button
          type="button"
          disabled
          className="flex items-center gap-1.5 rounded-md bg-muted px-3 py-1.5 text-xs font-medium text-muted-foreground"
        >
          <Loader2 className="size-3.5 animate-spin" />
          <Trans>下载中...</Trans>
        </button>
      );

    default:
      return (
        <button
          type="button"
          onClick={onCheck}
          className="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
        >
          <RefreshCw className="size-3.5" />
          <Trans>检查更新</Trans>
        </button>
      );
  }
}

/** 有更新可用时的蓝色 banner */
function UpdateBanner({
  latestVersion,
  releaseNotes,
}: {
  latestVersion: string | null;
  releaseNotes: string | null;
}) {
  return (
    <div className="flex flex-col gap-1.5 border-t border-blue-200 bg-blue-50 px-4 py-3 dark:border-blue-900 dark:bg-blue-950/50">
      <div className="flex items-center gap-2">
        <Sparkles className="size-3.5 text-blue-600 dark:text-blue-400" />
        <span className="text-[13px] font-semibold text-blue-700 dark:text-blue-300">
          {t`SwarmDrop v${latestVersion ?? "?"} 已发布`}
        </span>
      </div>
      {releaseNotes && (
        <div className="max-h-48 overflow-y-auto">
          <MarkdownContent
            content={releaseNotes}
            className="prose-headings:text-blue-700 dark:prose-headings:text-blue-300 text-blue-600 dark:text-blue-400"
          />
        </div>
      )}
    </div>
  );
}

/** 下载进度 banner */
function DownloadProgressBanner({
  latestVersion,
  progress,
}: {
  latestVersion: string | null;
  progress: UpdateProgress;
}) {
  // registry Progress.percent 是 0~1 分数，UI 用 0~100。
  const percent = Math.round(progress.percent * 100);
  return (
    <div className="flex flex-col gap-2.5 border-t border-border px-4 py-3.5">
      <div className="flex items-center justify-between">
        <span className="text-[13px] font-medium text-foreground">
          {t`正在下载 v${latestVersion ?? "?"}`}
        </span>
        <span className="text-[13px] font-semibold text-primary">
          {percent}%
        </span>
      </div>
      <Progress value={percent} className="h-1.5" />
      <div className="flex items-center justify-between">
        <span className="text-[11px] text-muted-foreground">
          {formatBytes(progress.downloaded)} / {formatBytes(progress.total)}
        </span>
        {progress.speed ? (
          <span className="text-[11px] text-muted-foreground">
            {formatBytes(progress.speed)}/s
          </span>
        ) : null}
      </div>
    </div>
  );
}
