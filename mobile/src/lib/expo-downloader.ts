import type { KeyValueStorage } from "@swarm-hive/sdk";
import * as FileSystem from "expo-file-system/legacy";
import { Platform } from "react-native";
import type {
  ApkArtifactExpectation,
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

/** 已落盘且校验通过的产物记录 —— 「磁盘上这个包是给哪一版的」。 */
const ARTIFACT_KEY = "swarmhive.apk_artifact";

/** 落盘产物的身份 —— 「磁盘上这个包是给哪一版的」。尺寸不存:复检时的期望值由候选
 *  release 提供,存一份旧的只会在服务端改了 size_bytes 时给出更差的判断。 */
interface ArtifactRecord {
  version: string;
  path: string;
}

export interface ExpoDownloaderOptions {
  /** KV 持久化 —— 断点存档与产物记录都存这里。与 adapter 用同一份实例。 */
  storage: KeyValueStorage;
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
 * APK 本质是 ZIP。OSS / CDN 错误页经常是 200 + XML/HTML —— downloadAsync 不抛错、状态也
 * 是 2xx,只有内容能识破。同一份判据在完成校验与 `reconcile` 复检里共用。
 */
async function hasZipMagic(uri: string): Promise<boolean> {
  const magic = await FileSystem.readAsStringAsync(uri, {
    encoding: FileSystem.EncodingType.Base64,
    position: 0,
    length: 4,
  });
  return magic.startsWith("UEs");
}

/**
 * 校验投递结果确实是一个完整的 APK,否则抛错(抛错即触发 adapter 的换源 fallback)。
 * 顺序:状态 → 非空 → 尺寸 → ZIP magic;尺寸是 O(1) 的 getInfoAsync,排在要读字节的
 * magic 之前。`expected.sizeBytes` 缺省时跳过尺寸这层,其余照常。
 */
async function assertApkDownload(
  result: FileSystem.FileSystemDownloadResult,
  expected: ApkArtifactExpectation,
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
  if (expected.sizeBytes != null && info.size !== expected.sizeBytes) {
    throw new Error(
      `APK download is truncated: expected ${expected.sizeBytes} bytes, got ${info.size}`,
    );
  }

  if (!(await hasZipMagic(result.uri))) {
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
 * 创建方案 A 的 ApkDownloader。
 *
 * `download(url, onProgress, expected?)`:
 *   清残留 → 全量下到 cacheDirectory → 校验是完整 APK → 记下产物版本 → resolve 本地
 *   `file://` 路径。校验失败先删残留文件与记录(不留毒化缓存)再抛。
 *   **不做断点续传** —— expo 的 resumeData 只有 pauseAsync 能产出,进程被杀时拿不到,
 *   理由见 download 内部注释。
 *
 * `reconcile(expected)`:把磁盘产物与候选对齐 —— 命中返回路径(SDK 据此跳过整个下载
 *   阶段),否则清理并返回 null。
 *
 * 非 Android 抛 `ApkDownloadNotSupportedOnIosError`。
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
  opts: ExpoDownloaderOptions,
): ApkDownloader {
  const { storage } = opts;
  const fileName = opts.fileName ?? "swarmhive-update.apk";

  function targetPath(): string {
    const cacheDir = FileSystem.cacheDirectory;
    if (!cacheDir) throw new Error("FileSystem.cacheDirectory unavailable");
    return `${cacheDir}${fileName}`;
  }

  async function readArtifact(): Promise<ArtifactRecord | null> {
    const raw = await storage.get(ARTIFACT_KEY);
    if (!raw) return null;
    try {
      return JSON.parse(raw) as ArtifactRecord;
    } catch {
      return null;
    }
  }

  /** 删文件 + 抹掉记录。产物失效的唯一收口,避免漏删其中一样。 */
  async function discardArtifact(path: string): Promise<void> {
    await FileSystem.deleteAsync(path, { idempotent: true });
    await storage.set(ARTIFACT_KEY, "");
  }

  return {
    async download(
      url: string,
      onProgress: ApkProgressCallback,
      expected: ApkArtifactExpectation,
    ): Promise<string> {
      if (Platform.OS !== "android") {
        throw new ApkDownloadNotSupportedOnIosError();
      }
      const target = targetPath();

      // 清掉上次残留的 partial 文件。
      //
      // **这里刻意不做断点续传。** expo 的 `DownloadResumable` 看起来支持它,实际不行:
      // `resumeData` 只在 `pauseAsync()` 里被赋值(`this.resumeData = pauseResult.resumeData`),
      // 而 `savable()` 只是把当前字段读出来。进程被杀时没有任何钩子能替我们调 pauseAsync,
      // 所以「下载开始前存 savable、下次用它 resume」存下来的 resumeData 恒为 undefined ——
      // 原生层收到空 resumeData 不会带 Range 头,反而会 truncate 目标文件,等于从头下,
      // 只是多留了个残留文件和一份假装有用的存档。
      //
      // 真要做需要 AppState 切后台时 pauseAsync + 立刻 resumeAsync 来刷出真实的 resumeData,
      // 那会打断后台下载(而「熄屏也要能下完」正是这套流程的诉求),得不偿失。
      // **已下载完成**的产物跨进程复用由 `reconcile` 负责,那条路是真的有效。
      await FileSystem.deleteAsync(target, { idempotent: true });

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
        await discardArtifact(result.uri);
        throw error;
      }

      const record: ArtifactRecord = {
        version: expected.version,
        path: result.uri,
      };
      await storage.set(ARTIFACT_KEY, JSON.stringify(record));
      return result.uri;
    },

    async reconcile(
      expected: ApkArtifactExpectation | null,
    ): Promise<string | null> {
      if (Platform.OS !== "android") return null;

      const record = await readArtifact();
      if (!record) {
        // 没有记录 ⇒ 没有可复用的产物。仍扫一遍固定路径:记录可能因存储清理丢了,
        // 文件却还占着缓存(端口契约要求返回 null 时不留无用残留)。
        await FileSystem.deleteAsync(targetPath(), { idempotent: true });
        return null;
      }
      if (!expected || record.version !== expected.version) {
        await discardArtifact(record.path);
        return null;
      }

      // 复检与下载完成时同一套判据:存在 → 尺寸 → ZIP magic。记录可能是上个进程写的,
      // 中间隔着一次系统缓存清理或用户手动删文件,不能只信记录。
      const info = await FileSystem.getInfoAsync(record.path);
      const usable =
        info.exists &&
        (expected.sizeBytes == null || info.size === expected.sizeBytes) &&
        (await hasZipMagic(record.path));
      if (!usable) {
        await discardArtifact(record.path);
        return null;
      }
      return record.path;
    },
  };
}
