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
 * - 优先用户在设置里选的 `receivePath`（iOS file://）
 * - Android SAF content:// 会被拒绝并自动清除：expo-file-system 55 的 next API
 *   在 SAF 目录下创建 / 写文件不稳定（dot 前缀文件被当作 folder、嵌套子目录创建
 *   失败等），实测会让接收卡 0%。这里做兜底：发现存量 content:// 直接回退到默认
 *   并把脏 receivePath 清掉，避免用户每次接收都踩坑。
 * - 未配置则退到应用私有 `transfersInboxUri`
 *
 * 调用方是同步代码（offer accept 回调），所以从 store 直接 getState 而不是 hook。
 */
export function resolveReceiveLocation(): string {
  const store = usePreferencesStore.getState();
  const custom = store.receivePath;
  if (custom?.startsWith("content://")) {
    store.setReceivePath(null);
    return getMobilePaths().transfersInboxUri;
  }
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
