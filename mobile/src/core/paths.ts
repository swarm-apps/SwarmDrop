import { Paths } from "expo-file-system";
import { usePreferencesStore } from "@/stores/preferences-store";

export type MobilePaths = {
  documentUri: string;
  cacheUri: string;
  transfersInboxUri: string;
};

export function getMobilePaths(): MobilePaths {
  return {
    documentUri: Paths.document.uri,
    cacheUri: Paths.cache.uri,
    transfersInboxUri: `${trimTrailingSlash(Paths.document.uri)}/transfers`,
  };
}

/**
 * 解析当前应使用的「接收保存目录」URI：
 * - 优先用户在设置里选的 `receivePath`（iOS file:// 或 Android SAF content://）
 * - 未配置则退到应用私有 `transfersInboxUri`
 *
 * SAF content:// 在 expo-file-system 56 起得到完整 chunk write 支持，由
 * `foreign-file-access.ts` 适配（见 `ensureSafSinkFile` + 持久 FileHandle）。
 *
 * 调用方是同步代码（offer accept 回调），所以从 store 直接 getState 而不是 hook。
 */
export function resolveReceiveLocation(): string {
  const custom = usePreferencesStore.getState().receivePath;
  return custom ?? getMobilePaths().transfersInboxUri;
}

export function fileUriToPath(uri: string): string {
  return uri.startsWith("file://")
    ? decodeURIComponent(uri.slice("file://".length))
    : uri;
}

function trimTrailingSlash(value: string): string {
  return value.endsWith("/") ? value.slice(0, -1) : value;
}
