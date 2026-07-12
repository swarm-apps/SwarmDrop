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
    <section className="border-b border-fd-border bg-fd-card/30">
      <div className="mx-auto grid w-full max-w-6xl items-center gap-8 px-6 py-12 lg:grid-cols-[0.9fr_1.1fr]">
        <div className="reveal">
          <span className="mb-4 inline-flex items-center gap-2 rounded-full border border-fd-border bg-fd-card px-3 py-1 text-xs font-medium text-fd-muted-foreground">
            <Smartphone className="size-3.5 text-[var(--brand)]" strokeWidth={2.25} />
            移动端 · Android
          </span>
          <h2 className="text-2xl font-bold tracking-tight sm:text-3xl">在手机上收发文件</h2>
          <p className="mt-3 max-w-md text-sm text-fd-muted-foreground">
            Android APK 与桌面端共用同一套 Rust 核心。下载项同样来自 SwarmHive stable channel,
            始终指向最新版本。
          </p>
        </div>

        <div className="reveal overflow-hidden rounded-2xl border border-fd-border bg-fd-card">
          {state === "idle" || state === "loading" ? (
            <div className="flex min-h-40 items-center justify-center p-8 text-sm text-fd-muted-foreground">
              <Loader2 className="mr-2 size-4 animate-spin text-[var(--brand)]" />
              正在读取 Android 下载目录
            </div>
          ) : !apk ? (
            <div className="flex min-h-40 flex-col justify-center gap-4 p-6 sm:p-8">
              <p className="text-sm text-fd-muted-foreground">
                stable 暂无公开 APK,可前往 GitHub Releases 获取最新构建。
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
            <div className="p-6 sm:p-8">
              <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
                <div className="min-w-0">
                  <p className="text-sm text-fd-muted-foreground">stable 最新版本</p>
                  <h3 className="mt-1 text-2xl font-bold tracking-tight">{catalog?.version}</h3>
                  <p className="mt-1 truncate text-xs text-fd-muted-foreground">{apk.filename}</p>
                </div>
                <a
                  href={apk.download_url}
                  className="inline-flex h-11 shrink-0 items-center justify-center gap-2 rounded-xl bg-[var(--brand-solid)] px-5 text-sm font-semibold text-[var(--brand-ink)] shadow-sm transition-all hover:opacity-90 hover:shadow-md active:scale-[0.98]"
                >
                  下载 APK
                  <ArrowRight className="size-4" />
                </a>
              </div>
              {mirror ? (
                <a
                  href={mirror}
                  className="mt-4 inline-flex items-center gap-1.5 text-xs font-medium text-fd-muted-foreground transition-colors hover:text-fd-foreground"
                >
                  <Github className="size-3.5" />
                  从 GitHub Release 镜像下载(海外 / 备用)
                </a>
              ) : null}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
