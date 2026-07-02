"use client";

import {
  type DownloadArtifact,
  type DownloadCatalog,
  getDownloadCatalog,
  selectBestDownload,
} from "@swarm-hive/sdk";
import {
  ArrowRight,
  Download,
  ExternalLink,
  Loader2,
  MonitorDown,
  PackageOpen,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

type LoadState = "idle" | "loading" | "ready" | "error";

interface DownloadPanelProps {
  baseUrl: string;
  appSlug: string;
  channel: string;
  fallbackUrl: string;
  initialCatalog?: DownloadCatalog | null;
  allowClientRefresh?: boolean;
}

const FALLBACK_TEXT = "前往 GitHub Releases";

export function DownloadPanel({
  baseUrl,
  appSlug,
  channel,
  fallbackUrl,
  initialCatalog,
  allowClientRefresh = true,
}: DownloadPanelProps) {
  const shouldUseClientFetch = initialCatalog === undefined && allowClientRefresh;
  const [catalog, setCatalog] = useState<DownloadCatalog | null>(initialCatalog ?? null);
  const [state, setState] = useState<LoadState>(
    initialCatalog ? "ready" : shouldUseClientFetch ? "idle" : "error",
  );
  const [error, setError] = useState<unknown>(
    initialCatalog || shouldUseClientFetch
      ? null
      : new Error("当前构建未读取到 SwarmHive 下载目录。"),
  );

  const load = useCallback(
    async (isCancelled?: () => boolean) => {
      setState("loading");
      setError(null);
      try {
        const next = await getDownloadCatalog({ baseUrl, appSlug, channel });
        if (!isCancelled?.()) {
          setCatalog(next);
          setState("ready");
        }
      } catch (cause) {
        if (!isCancelled?.()) {
          setCatalog(null);
          setError(cause);
          setState("error");
        }
      }
    },
    [appSlug, baseUrl, channel],
  );

  useEffect(() => {
    if (!shouldUseClientFetch) return;

    let cancelled = false;
    void load(() => cancelled);
    return () => {
      cancelled = true;
    };
  }, [load, shouldUseClientFetch]);

  const primary = useMemo(() => {
    if (!catalog) return null;
    return (
      selectBestDownload(catalog) ?? (catalog.artifacts.length === 1 ? catalog.artifacts[0] : null)
    );
  }, [catalog]);

  return (
    <section id="download" className="border-y border-fd-border bg-fd-card/30 scroll-mt-20">
      <div className="mx-auto grid w-full max-w-6xl gap-8 px-6 py-16 lg:grid-cols-[0.9fr_1.1fr] lg:items-start">
        <div className="reveal">
          <span className="mb-4 inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-3 py-1 text-xs font-medium text-fd-muted-foreground">
            <Download className="size-3.5 text-[var(--brand)]" strokeWidth={2.25} />
            公开下载
          </span>
          <h2 className="text-3xl font-bold tracking-tight sm:text-4xl">
            自动匹配你的平台安装包
          </h2>
          <p className="mt-4 max-w-md text-fd-muted-foreground">
            官网下载项直接来自 SwarmHive stable channel。桌面端显示安装包，应用内更新仍走独立 updater
            端点。
          </p>
        </div>

        <div className="reveal overflow-hidden rounded-2xl border border-fd-border bg-fd-card">
          <DownloadPanelBody
            state={state}
            catalog={catalog}
            primary={primary}
            error={error}
            fallbackUrl={fallbackUrl}
            onRetry={allowClientRefresh ? () => void load() : undefined}
          />
        </div>
      </div>
    </section>
  );
}

function DownloadPanelBody({
  state,
  catalog,
  primary,
  error,
  fallbackUrl,
  onRetry,
}: {
  state: LoadState;
  catalog: DownloadCatalog | null;
  primary: DownloadArtifact | null;
  error: unknown;
  fallbackUrl: string;
  onRetry?: () => void;
}) {
  if (state === "idle" || state === "loading") {
    return (
      <div className="flex min-h-72 items-center justify-center p-8 text-sm text-fd-muted-foreground">
        <Loader2 className="mr-2 size-4 animate-spin text-[var(--brand)]" />
        正在读取 stable 下载目录
      </div>
    );
  }

  if (state === "error") {
    return (
      <div className="flex min-h-72 flex-col justify-center gap-5 p-6 sm:p-8">
        <div className="flex items-start gap-3">
          <PackageOpen className="mt-0.5 size-5 shrink-0 text-[var(--brand)]" />
          <div>
            <h3 className="font-semibold">暂时无法读取 SwarmHive 下载目录</h3>
            <p className="mt-1 text-sm leading-relaxed text-fd-muted-foreground">
              {error instanceof Error ? error.message : "请稍后重试，或使用 GitHub Releases。"}
            </p>
          </div>
        </div>
        <div className="flex flex-wrap gap-3">
          {onRetry ? (
            <button
              type="button"
              onClick={onRetry}
              className="inline-flex h-10 items-center justify-center rounded-xl border border-fd-border px-4 text-sm font-semibold transition-colors hover:bg-fd-accent"
            >
              重试
            </button>
          ) : null}
          <FallbackLink href={fallbackUrl} />
        </div>
      </div>
    );
  }

  if (!catalog || catalog.artifacts.length === 0) {
    return (
      <div className="flex min-h-72 flex-col justify-center gap-5 p-6 sm:p-8">
        <div className="flex items-start gap-3">
          <PackageOpen className="mt-0.5 size-5 shrink-0 text-[var(--brand)]" />
          <div>
            <h3 className="font-semibold">stable 版本暂无公开安装包</h3>
            <p className="mt-1 text-sm leading-relaxed text-fd-muted-foreground">
              可以先从 GitHub Releases 获取最新构建。
            </p>
          </div>
        </div>
        <FallbackLink href={fallbackUrl} />
      </div>
    );
  }

  return (
    <div>
      <div className="border-b border-fd-border p-6 sm:p-8">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <p className="text-sm text-fd-muted-foreground">stable 最新版本</p>
            <h3 className="mt-1 text-2xl font-bold tracking-tight">{catalog.version}</h3>
          </div>
          {primary ? (
            <a
              href={primary.download_url}
              className="inline-flex h-11 shrink-0 items-center justify-center gap-2 rounded-xl bg-[var(--brand-solid)] px-5 text-sm font-semibold text-[var(--brand-ink)] shadow-sm transition-all hover:opacity-90 hover:shadow-md active:scale-[0.98]"
            >
              下载推荐版本
              <ArrowRight className="size-4" />
            </a>
          ) : (
            <FallbackLink href={fallbackUrl} />
          )}
        </div>
        {primary ? (
          <p className="mt-3 truncate text-sm text-fd-muted-foreground">{primary.filename}</p>
        ) : null}
      </div>

      <div className="divide-y divide-fd-border">
        {catalog.artifacts.map((artifact) => (
          <DownloadRow key={artifact.id} artifact={artifact} />
        ))}
      </div>
    </div>
  );
}

function DownloadRow({ artifact }: { artifact: DownloadArtifact }) {
  return (
    <a
      href={artifact.download_url}
      className="grid gap-3 p-4 text-sm transition-colors hover:bg-fd-accent/60 sm:grid-cols-[1fr_auto] sm:items-center sm:px-6"
    >
      <span className="min-w-0">
        <span className="flex min-w-0 items-center gap-2 font-medium">
          <MonitorDown className="size-4 shrink-0 text-[var(--brand)]" />
          <span className="truncate">{artifact.filename}</span>
        </span>
        <span className="mt-1 block truncate text-xs text-fd-muted-foreground">
          {variantLabel(artifact)} · {formatBytes(artifact.size_bytes)} · sha256{" "}
          {artifact.sha256.slice(0, 12)}
        </span>
      </span>
      <span className="flex items-center gap-2 text-xs font-medium text-fd-muted-foreground">
        {artifact.kind}
        <ExternalLink className="size-3.5" />
      </span>
    </a>
  );
}

function FallbackLink({ href }: { href: string }) {
  return (
    <a
      href={href}
      className="inline-flex h-10 w-fit items-center justify-center gap-2 rounded-xl bg-[var(--brand-solid)] px-4 text-sm font-semibold text-[var(--brand-ink)] transition-all hover:opacity-90 active:scale-[0.98]"
    >
      {FALLBACK_TEXT}
      <ExternalLink className="size-4" />
    </a>
  );
}

function variantLabel(artifact: DownloadArtifact): string {
  if (artifact.target) return artifact.target;
  if (artifact.arch) return artifact.arch;
  if (artifact.abi) return artifact.abi;
  return artifact.platform;
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"] as const;
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}
