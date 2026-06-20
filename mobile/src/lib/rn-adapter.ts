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
      const tracker = new DownloadSpeedTracker(onProgress);
      const apkPath = await opts.downloader.download(
        release.url,
        (downloaded, total) => {
          tracker.update(downloaded, total);
        },
      );
      tracker.finish();
      // payload 必须 self-contained(engine install 前会清 pendingHandle):存本地 APK 路径。
      return { release, payload: apkPath };
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
