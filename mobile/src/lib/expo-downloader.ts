import * as FileSystem from "expo-file-system/legacy";
import { Platform } from "react-native";
import type { ApkDownloader, ApkProgressCallback } from "./ports";

/** iOS / 非 Android 平台不支持下载安装 APK。 */
export class ApkDownloadNotSupportedOnIosError extends Error {
  constructor() {
    super("APK download is not supported on iOS");
    this.name = "ApkDownloadNotSupportedOnIosError";
  }
}

export interface ExpoDownloaderOptions {
  /** 缓存文件名,默认 swarmhive-update.apk(放在 FileSystem.cacheDirectory 下)。 */
  fileName?: string;
}

/**
 * 创建方案 A 的 ApkDownloader。download(url, onProgress):
 *   清理上次残留 → createDownloadResumable 下到 cacheDirectory → resolve 本地 file:// 路径。
 * 非 Android 抛 ApkDownloadNotSupportedOnIosError。
 */
export function createExpoApkDownloader(
  opts: ExpoDownloaderOptions = {},
): ApkDownloader {
  const fileName = opts.fileName ?? "swarmhive-update.apk";
  return {
    async download(
      url: string,
      onProgress: ApkProgressCallback,
    ): Promise<string> {
      if (Platform.OS !== "android") {
        throw new ApkDownloadNotSupportedOnIosError();
      }
      const cacheDir = FileSystem.cacheDirectory;
      if (!cacheDir) throw new Error("FileSystem.cacheDirectory unavailable");
      const target = `${cacheDir}${fileName}`;

      // 清掉上次残留的 partial 文件,避免 resume 冲突。
      const info = await FileSystem.getInfoAsync(target);
      if (info.exists) {
        await FileSystem.deleteAsync(target, { idempotent: true });
      }

      const resumable = FileSystem.createDownloadResumable(
        url,
        target,
        {},
        (p) => {
          // 累计绝对值直接透传给 adapter 的 DownloadSpeedTracker(它负责节流 + percent)。
          onProgress(p.totalBytesWritten, p.totalBytesExpectedToWrite);
        },
      );

      const result = await resumable.downloadAsync();
      if (!result?.uri) {
        throw new Error("Download produced no file");
      }
      return result.uri;
    },
  };
}
