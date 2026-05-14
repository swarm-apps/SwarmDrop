import { Paths } from "expo-file-system";

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

export function fileUriToPath(uri: string): string {
  return uri.startsWith("file://")
    ? decodeURIComponent(uri.slice("file://".length))
    : uri;
}

function trimTrailingSlash(value: string): string {
  return value.endsWith("/") ? value.slice(0, -1) : value;
}
