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

function getHeader(
  headers: Record<string, string> | undefined,
  name: string,
): string | undefined {
  if (!headers) return undefined;
  const needle = name.toLowerCase();
  for (const [key, value] of Object.entries(headers)) {
    if (key.toLowerCase() === needle) return value;
  }
  return undefined;
}

async function readTextPreview(uri: string): Promise<string> {
  try {
    const text = await FileSystem.readAsStringAsync(uri, {
      encoding: FileSystem.EncodingType.UTF8,
    });
    return text.replace(/\s+/g, " ").slice(0, 160);
  } catch {
    return "";
  }
}

async function assertApkDownload(
  result: FileSystem.FileSystemDownloadResult,
): Promise<void> {
  if (result.status < 200 || result.status >= 300) {
    const contentType = getHeader(result.headers, "content-type");
    const preview = await readTextPreview(result.uri);
    throw new Error(
      `APK download returned HTTP ${result.status}${
        contentType ? ` (${contentType})` : ""
      }${preview ? `: ${preview}` : ""}`,
    );
  }

  const info = await FileSystem.getInfoAsync(result.uri);
  if (!info.exists || (info.size ?? 0) < 4) {
    throw new Error("APK download produced an empty file");
  }

  // APK 本质是 ZIP。OSS / CDN 错误页经常是 200/400 + XML/HTML,这里在安装前拦截。
  const magic = await FileSystem.readAsStringAsync(result.uri, {
    encoding: FileSystem.EncodingType.Base64,
    position: 0,
    length: 4,
  });
  if (!magic.startsWith("UEs")) {
    const contentType = getHeader(result.headers, "content-type");
    throw new Error(
      `Downloaded file is not an APK${contentType ? ` (${contentType})` : ""}`,
    );
  }
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
      try {
        await assertApkDownload(result);
      } catch (error) {
        await FileSystem.deleteAsync(result.uri, { idempotent: true });
        throw error;
      }
      return result.uri;
    },
  };
}
