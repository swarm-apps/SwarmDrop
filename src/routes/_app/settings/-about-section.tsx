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
  Bot,
  Download,
  ExternalLink,
  Github,
  Globe2,
  Info,
  KeyRound,
  Loader2,
  RefreshCw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useEffect, useState, type ComponentType, type ReactNode } from "react";
import { MarkdownContent } from "@/components/ui/markdown-content";
import { Progress } from "@/components/ui/progress";
import { useUpdate } from "@/hooks/use-update";
import { SettingsCard, SettingsSection } from "./-settings-primitives";

/** 格式化字节数为人类可读 */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function AboutSection() {
  return (
    <SettingsSection title={<Trans>关于</Trans>} icon={Info}>
      <AboutPanel />
    </SettingsSection>
  );
}

export function AboutPanel({ className }: { className?: string }) {
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

  return (
    <SettingsCard className={className}>
      <div className="flex flex-col gap-5 p-4 sm:p-5">
        {/* 品牌信息 + 操作按钮 */}
        <div className="flex flex-col gap-4 min-[640px]:flex-row min-[640px]:items-start min-[640px]:justify-between">
          <div className="flex gap-3.5">
            <img
              src="/app-icon.svg"
              alt="SwarmDrop"
              className="size-12 shrink-0 rounded-2xl"
            />
            <div className="flex min-w-0 flex-col gap-1.5">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-base font-semibold text-foreground">
                  SwarmDrop
                </span>
                {appVersion ? (
                  <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-brand">
                    v{appVersion}
                  </span>
                ) : null}
              </div>
              <p className="text-[13px] font-medium text-foreground/90">
                <Trans>设备之间的数据通道 —— 人与 AI 代理皆可用</Trans>
              </p>
              <p className="hidden max-w-md text-xs leading-5 text-muted-foreground min-[480px]:block">
                <Trans>
                  不止于局域网：在任意网络间端到端加密地收发文件，连 AI 代理也能经
                  MCP 调用。
                </Trans>
              </p>
            </div>
          </div>

          {/* 按钮组 */}
          <div className="flex flex-wrap items-center gap-2 min-[640px]:shrink-0 min-[640px]:justify-end">
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

        {/* 核心特性 */}
        <div className="flex flex-wrap gap-1.5">
          <FeatureTag icon={Globe2} label={<Trans>跨网络</Trans>} />
          <FeatureTag icon={ShieldCheck} label={<Trans>端到端加密</Trans>} />
          <FeatureTag icon={KeyRound} label={<Trans>无账户 · 无服务器</Trans>} />
          <FeatureTag icon={Bot} label={<Trans>AI 原生 · MCP</Trans>} />
          <FeatureTag icon={RefreshCw} label={<Trans>断点续传</Trans>} />
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
    </SettingsCard>
  );
}

/** 核心特性小标签 */
function FeatureTag({
  icon: Icon,
  label,
}: {
  icon: ComponentType<{ className?: string }>;
  label: ReactNode;
}) {
  return (
    <span className="flex items-center gap-1.5 rounded-full border border-border/60 bg-background/50 px-2.5 py-1 text-[11px] font-medium text-muted-foreground dark:bg-white/[0.03]">
      <Icon className="size-3.5 text-brand" />
      {label}
    </span>
  );
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
      className="flex items-center gap-1.5 rounded-lg border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-accent"
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
          className="flex items-center gap-1.5 rounded-lg bg-primary/50 px-3 py-1.5 text-xs font-medium text-primary-foreground"
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
          className="flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
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
          className="flex items-center gap-1.5 rounded-lg bg-muted px-3 py-1.5 text-xs font-medium text-muted-foreground"
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
          className="flex items-center gap-1.5 rounded-lg bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
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
    <div className="flex flex-col gap-1.5 border-t border-primary/25 bg-primary/10 px-4 py-3 dark:border-primary/25 dark:bg-primary/10">
      <div className="flex items-center gap-2">
        <Sparkles className="size-3.5 text-brand" />
        <span className="text-[13px] font-semibold text-brand">
          {t`SwarmDrop v${latestVersion ?? "?"} 已发布`}
        </span>
      </div>
      {releaseNotes && (
        <div className="max-h-48 overflow-y-auto">
          <MarkdownContent
            content={releaseNotes}
            className="prose-headings:text-brand dark:prose-headings:text-brand text-brand"
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
        <span className="text-[13px] font-semibold text-brand">
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
