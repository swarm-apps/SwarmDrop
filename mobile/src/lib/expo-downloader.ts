import * as FileSystem from "expo-file-system/legacy";
import { Platform } from "react-native";
import type {
  ApkDownloadExpectation,
  ApkDownloader,
  ApkProgressCallback,
} from "./ports";

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

/**
 * 校验投递结果确实是一个完整的 APK,否则抛错(抛错即触发 adapter 的换源 fallback)。
 * 顺序:状态 → 非空 → 尺寸 → ZIP magic;尺寸是 O(1) 的 getInfoAsync,排在要读字节的
 * magic 之前。`expected` 缺省时跳过尺寸这层,其余照常。
 */
async function assertApkDownload(
  result: FileSystem.FileSystemDownloadResult,
  expected?: ApkDownloadExpectation,
): Promise<void> {
  if (result.status < 200 || result.status >= 300) {
    const contentType = getHeader(result.headers, "content-type");
    const preview = await readTextPreview(result.uri);
    throw new Error(
      `APK download returned HTTP ${result.status}${contentType ? ` (${contentType})` : ""}${
        preview ? `: ${preview}` : ""
      }`,
    );
  }

  const info = await FileSystem.getInfoAsync(result.uri);
  if (!info.exists || info.size < 4) {
    throw new Error("APK download produced an empty file");
  }

  // 截断投递:连接中断后 downloadAsync 照常 resolve,残缺文件的 ZIP magic 仍然合法,
  // 只有尺寸能发现。
  if (expected?.sizeBytes != null && info.size !== expected.sizeBytes) {
    throw new Error(
      `APK download is truncated: expected ${expected.sizeBytes} bytes, got ${info.size}`,
    );
  }

  // APK 本质是 ZIP。OSS / CDN 错误页经常是 200 + XML/HTML —— downloadAsync 不抛错、状态也
  // 是 2xx,只有内容能识破,这里在安装前拦截。
  const magic = await FileSystem.readAsStringAsync(result.uri, {
    encoding: FileSystem.EncodingType.Base64,
    position: 0,
    length: 4,
  });
  if (!magic.startsWith("UEs")) {
    const contentType = getHeader(result.headers, "content-type");
    const preview = await readTextPreview(result.uri);
    throw new Error(
      `Downloaded file is not an APK${contentType ? ` (${contentType})` : ""}${
        preview ? `: ${preview}` : ""
      }`,
    );
  }
}

/**
 * 创建方案 A 的 ApkDownloader。download(url, onProgress, expected?):
 *   清理上次残留 → createDownloadResumable 下到 cacheDirectory → 校验是完整 APK →
 *   resolve 本地 file:// 路径。校验失败先删残留文件(不留毒化缓存给下次 resume)再抛。
 * 非 Android 抛 ApkDownloadNotSupportedOnIosError。
 *
 * ⚠️ **本文件由 `@swarmhive-rn` registry 分发,上游在 SwarmHive
 * `packages/registry-rn/registry/rn/lib/expo-downloader.ts`。要改请改上游再重新拉取**
 * —— 就地改会在下次拉取时被覆盖,且改动不会回流给其它 app。
 *
 * 这条声明刻意放在 JSDoc 而非文件头 banner:shadcn 拉取时会**剥掉 banner**,放那里等于
 * 只有上游看得见、下游看不见 —— 而下游正是需要看到它的人。上下游倒置(上游自称下游的
 * 镜像)曾让下游加的 APK 校验没有回流义务,registry 于是给每个新装配的 app 发了一个不
 * 设防的下载器,把 OSS 的 XML 错误页当 APK 喂给系统安装器(见 harden-rn-apk-downloader)。
 */
export function createExpoApkDownloader(
  opts: ExpoDownloaderOptions = {},
): ApkDownloader {
  const fileName = opts.fileName ?? "swarmhive-update.apk";
  return {
    async download(
      url: string,
      onProgress: ApkProgressCallback,
      expected?: ApkDownloadExpectation,
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
        await assertApkDownload(result, expected);
      } catch (error) {
        await FileSystem.deleteAsync(result.uri, { idempotent: true });
        throw error;
      }
      return result.uri;
    },
  };
}
