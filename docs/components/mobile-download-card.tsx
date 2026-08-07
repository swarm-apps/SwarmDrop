"use client";

import {
  type DownloadArtifact,
  type DownloadCatalog,
  getDownloadCatalog,
} from "@swarm-hive/sdk";
import { ArrowRight, ExternalLink, Github, Loader2, Smartphone } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

type LoadState = "idle" | "loading" | "ready" | "error";

interface MobileDownloadCardProps {
  baseUrl: string;
  appSlug: string;
  channel: string;
  /** GitHub Releases 兜底链接。 */
  fallbackUrl: string;
  initialCatalog?: DownloadCatalog | null;
  allowClientRefresh?: boolean;
}

/** 取 APK:优先 installer/universal,退回第一个 artifact。 */
function pickApk(catalog: DownloadCatalog): DownloadArtifact | null {
  return (
    catalog.artifacts.find((a) => a.kind === "installer" || a.kind === "universal") ??
    catalog.artifacts[0] ??
    null
  );
}

// `sources`(多下载源)是 SDK 0.3.0 起才有的字段;这里按本地形状读取,兼容站点当前
// 钉的 0.2.0 类型 —— 服务端(SwarmHive ≥ 0.7.0)运行时总会带上它。
function githubSource(artifact: DownloadArtifact): string | null {
  const sources = (artifact as { sources?: { kind: string; url: string }[] }).sources;
  return sources?.find((s) => s.kind === "github")?.url ?? null;
}

/**
 * 移动端(Android)下载入口。与桌面下载面板一样,运行时向 SwarmHive 拉 `swarmdrop-rn`
 * 的公开目录 —— 目录返回的永远是 stable channel 当前 release,所以发版后官网零改动、
 * 零重建就自动指向最新 APK。
 */
export function MobileDownloadCard({
  baseUrl,
  appSlug,
  channel,
  fallbackUrl,
  initialCatalog,
  allowClientRefresh = true,
}: MobileDownloadCardProps) {
  const shouldUseClientFetch = initialCatalog === undefined && allowClientRefresh;
  const [catalog, setCatalog] = useState<DownloadCatalog | null>(initialCatalog ?? null);
  const [state, setState] = useState<LoadState>(
    initialCatalog ? "ready" : shouldUseClientFetch ? "idle" : "error",
  );

  const load = useCallback(
    async (isCancelled?: () => boolean) => {
      setState("loading");
      try {
        const next = await getDownloadCatalog({ baseUrl, appSlug, channel });
        if (!isCancelled?.()) {
          setCatalog(next);
          setState("ready");
        }
      } catch {
        if (!isCancelled?.()) {
          setCatalog(null);
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

  const apk = useMemo(() => (catalog ? pickApk(catalog) : null), [catalog]);
  const mirror = apk ? githubSource(apk) : null;

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-2xl border border-fd-border bg-fd-card">
      <div className="flex items-center gap-2 border-b border-fd-border px-6 py-3.5 text-sm font-medium">
        <Smartphone className="size-4 text-[var(--brand)]" strokeWidth={2.25} />
        Android
      </div>

      {state === "idle" || state === "loading" ? (
        <div className="flex flex-1 items-center justify-center p-8 text-sm text-fd-muted-foreground">
          <Loader2 className="mr-2 size-4 animate-spin text-[var(--brand)]" />
          正在读取 Android 下载目录
        </div>
      ) : !apk ? (
        <div className="flex flex-1 flex-col justify-center gap-4 p-6">
          <p className="text-sm text-fd-muted-foreground">
            stable 暂无公开 APK，可前往 GitHub Releases 获取最新构建。
          </p>
          <a
            href={fallbackUrl}
            className="inline-flex h-10 w-fit items-center justify-center gap-2 rounded-xl bg-[var(--brand-solid)] px-4 text-sm font-semibold text-[var(--brand-ink)] transition-all hover:opacity-90 active:scale-[0.98]"
          >
            前往 GitHub Releases
            <ExternalLink className="size-4" />
          </a>
        </div>
      ) : (
        <div className="flex flex-1 flex-col p-6">
          <p className="text-sm text-fd-muted-foreground">stable 最新版本</p>
          <h3 className="mt-1 text-2xl font-bold tracking-tight">{catalog?.version}</h3>
          <p className="mt-1 truncate text-xs text-fd-muted-foreground">{apk.filename}</p>

          <a
            href={apk.download_url}
            className="mt-5 inline-flex h-11 items-center justify-center gap-2 rounded-xl bg-[var(--brand-solid)] px-5 text-sm font-semibold text-[var(--brand-ink)] shadow-sm transition-all hover:opacity-90 hover:shadow-md active:scale-[0.98]"
          >
            下载 APK
            <ArrowRight className="size-4" />
          </a>

          {mirror ? (
            <a
              href={mirror}
              className="mt-4 inline-flex items-center gap-1.5 text-xs font-medium text-fd-muted-foreground transition-colors hover:text-fd-foreground"
            >
              <Github className="size-3.5" />
              GitHub Release 镜像（海外 / 备用）
            </a>
          ) : null}
        </div>
      )}
    </div>
  );
}
