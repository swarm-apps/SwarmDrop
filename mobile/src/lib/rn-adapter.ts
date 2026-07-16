import {
  type CheckContext,
  checkUpdateAndroid,
  type DownloadHandle,
  type KeyValueStorage,
  type Progress,
  type ReleaseInfo,
  type UpdateAdapter,
  versionCodeComparator,
} from "@swarm-hive/sdk";
import type { ApkDownloader, ApkInstaller } from "./ports";

/**
 * 下载进度 + 瞬时速度跟踪(500ms 节流)。搬自 tauri-adapter 的 DownloadSpeedTracker,
 * 改成累计式接口:RN 的 createDownloadResumable 回调直接给「已下载/总量」,
 * 故用 `update(downloaded,total)` 取代 Tauri 的 `started/progress(chunk)`。
 */
class DownloadSpeedTracker {
  private total = 0;
  private downloaded = 0;
  private lastEmitAt = 0;
  private lastEmitBytes = 0;
  private speed = 0;
  private emitted = false;

  constructor(
    private readonly onProgress: (p: Progress) => void,
    private readonly throttleMs = 500,
  ) {}

  /** 累计进度回调(downloaded/total 为累计绝对值)。首帧立即发,其后 500ms 节流。 */
  update(downloaded: number, total: number): void {
    this.downloaded = downloaded;
    this.total = total;
    const now = Date.now();
    if (!this.emitted) {
      this.lastEmitAt = now;
      this.lastEmitBytes = downloaded;
      this.emit();
      return;
    }
    const dt = now - this.lastEmitAt;
    if (dt >= this.throttleMs) {
      this.speed = ((this.downloaded - this.lastEmitBytes) * 1000) / dt;
      this.lastEmitAt = now;
      this.lastEmitBytes = this.downloaded;
      this.emit();
    }
  }

  /** 下载完成收口:服务端没给 total 时用已下载量兜底,保证最终 percent = 1。 */
  finish(): void {
    if (this.total === 0) this.total = this.downloaded;
    this.emit(1);
  }

  private emit(percentOverride?: number): void {
    this.emitted = true;
    const percent =
      percentOverride ?? (this.total > 0 ? this.downloaded / this.total : 0);
    this.onProgress({
      downloaded: this.downloaded,
      total: this.total,
      percent,
      speed: this.speed || undefined,
    });
  }
}

export interface RnAdapterOptions {
  /** SwarmHive server base URL(如 https://hive.example.com)。 */
  baseUrl: string;
  /** App slug。 */
  appSlug: string;
  /** 当前 versionName(显示用;透传给 endpoint 的 current_version_name)。 */
  currentVersionName: string;
  /** 设备 ABI(arm64-v8a / armeabi-v7a / x86_64);缺省让 server 走 fat APK/单产物兜底。 */
  abi?: string;
  /** 可选 channel;缺省走 app 默认 channel。 */
  channel?: string;
  /** APK 下载器(注入;真实实现见 expo-downloader.ts)。 */
  downloader: ApkDownloader;
  /** APK 安装器(注入;真实实现见 expo-installer.ts)。 */
  installer: ApkInstaller;
  /** KV 持久化(注入;真实实现见 storage.ts)。 */
  storage: KeyValueStorage;
  /** 注入 fetch(测试/RN polyfill);默认全局 fetch,透传给 checkUpdateAndroid。 */
  fetchImpl?: typeof fetch;
}

/**
 * 创建 RN/Expo Android 平台的 UpdateAdapter。
 *
 * - check:把 SDK 的 `CheckContext.currentVersion`(engine 里是 versionCode 的字符串形式)
 *   转 Number 当 currentVersionCode,clientId 透传,委托 `checkUpdateAndroid`。
 * - download:用注入的 downloader 下 `release.url`,DownloadSpeedTracker 产出 SDK Progress,
 *   payload = 本地 APK 路径(string)。
 * - install:把 payload(APK 路径)交给注入的 installer,fire-and-forget,**不 relaunch**。
 */
export function createRnAdapter(opts: RnAdapterOptions): UpdateAdapter {
  return {
    storage: opts.storage,
    compare: versionCodeComparator,

    async check(ctx: CheckContext): Promise<ReleaseInfo | null> {
      return checkUpdateAndroid({
        baseUrl: opts.baseUrl,
        appSlug: opts.appSlug,
        // engine 用 versionCode 的字符串形式存 currentVersion;转回整数喂 endpoint。
        currentVersionCode: Number(ctx.currentVersion),
        currentVersionName: opts.currentVersionName,
        abi: opts.abi,
        channel: opts.channel,
        clientId: ctx.clientId,
        fetchImpl: opts.fetchImpl,
      });
    },

    async download(
      release: ReleaseInfo,
      onProgress: (p: Progress) => void,
    ): Promise<DownloadHandle> {
      // 主源 + 备用源(GitHub Release,已过服务端 liveness/digest 校验)按序尝试:
      // 某源投递失败(OSS 匿名下 APK 受限的错误页 / 截断 / 网络错误)则逐个 fallback
      // (`add-github-release-source`)。
      //
      // 触发 fallback 的判据 = 注入的 downloader 抛错 —— 校验归下载器所有(碰 expo-* 的是
      // 它,本 adapter 因此保持纯逻辑可单测),这里只把期望值传下去。客户端 sha256 校验的
      // 要求已被撤销(`harden-rn-apk-downloader` design D5):Expo 没有廉价的流式 SHA-256、
      // 服务端在暴露镜像前已比对过 digest、传输损坏由尺寸 + ZIP magic 拦下、APK 真伪由
      // Android 安装器验签兜底。
      const candidates = [release.url, ...(release.mirrorUrls ?? [])].filter(
        (u, i, arr): u is string => !!u && arr.indexOf(u) === i,
      );
      let lastErr: unknown;
      for (const url of candidates) {
        const tracker = new DownloadSpeedTracker(onProgress);
        try {
          const apkPath = await opts.downloader.download(
            url,
            (downloaded, total) => {
              tracker.update(downloaded, total);
            },
            { sizeBytes: release.sizeBytes },
          );
          tracker.finish();
          // payload 必须 self-contained(engine install 前会清 pendingHandle):存本地 APK 路径。
          return { release, payload: apkPath };
        } catch (e) {
          lastErr = e;
        }
      }
      throw lastErr ?? new Error("no download source available");
    },

    async install(handle: DownloadHandle): Promise<void> {
      const apkPath = handle.payload;
      if (typeof apkPath !== "string" || !apkPath) {
        throw new Error(
          "no downloaded APK path — call download() before install()",
        );
      }
      // fire-and-forget handoff:交给系统 PackageInstaller,intent 派发即 resolve。
      // **绝不 relaunch** —— RN 由系统替换进程,用户确认后旧进程被新版本接管。
      await opts.installer.install(apkPath);
    },
  };
}
