import type {
  EnumeratedFile,
  InboxItemFileEntry,
  TransferOfferFileEvent,
  TransferProgressEvent,
  TransferProjectionFile,
} from "@/lib/bindings";
import type { FileBrowserItem, FileBrowserStatus } from "./types";
import { normalizeRelativePath } from "./tree-data";

function stableId(scope: string, value: string | number): string {
  return `${scope}:${String(value).replace(/\\/g, "/")}`;
}

export function fromEnumeratedFiles(
  files: EnumeratedFile[],
): FileBrowserItem[] {
  return files.map((file) => {
    const relativePath = normalizeRelativePath(file.relativePath || file.name);
    return {
      id: stableId("send", file.source.path || relativePath),
      name: file.name,
      relativePath,
      size: file.size,
      localPath: file.source.path,
      status: "idle",
    };
  });
}

export function fromOfferFiles(
  files: TransferOfferFileEvent[],
): FileBrowserItem[] {
  return files
    .filter((file) => !file.isDirectory)
    .map((file) => ({
      id: stableId("offer", file.fileId),
      fileId: file.fileId,
      name: file.name,
      relativePath: normalizeRelativePath(file.relativePath || file.name),
      size: file.size,
      // Offer 尚未进入传输队列，只作为只读内容预览，不显示逐文件等待图标。
      status: "idle",
    }));
}

export interface TransferAdapterOptions {
  progress?: TransferProgressEvent | null;
  completedFileIds?: ReadonlySet<number>;
  errorFileIds?: ReadonlySet<number>;
  defaultStatus?: FileBrowserStatus;
}

export function fromTransferProjectionFiles(
  files: TransferProjectionFile[],
  options: TransferAdapterOptions = {},
): FileBrowserItem[] {
  const progressById = new Map(
    options.progress?.files.map((file) => [file.fileId, file]) ?? [],
  );

  return files.map((file) => {
    const live = progressById.get(file.fileId);
    let status = options.defaultStatus ?? "waiting";
    if (options.errorFileIds?.has(file.fileId)) status = "error";
    else if (options.completedFileIds?.has(file.fileId)) status = "completed";
    else if (live?.status === "completed") status = "completed";
    else if (live?.status === "transferring") status = "transferring";

    const transferred = live?.transferred ?? file.transferredBytes;
    const progress = file.size > 0
      ? Math.min(100, Math.max(0, (transferred / file.size) * 100))
      : status === "completed" ? 100 : 0;

    return {
      id: stableId("transfer", file.fileId),
      fileId: file.fileId,
      name: file.name,
      relativePath: normalizeRelativePath(file.relativePath || file.name),
      size: file.size,
      status,
      progress,
    };
  });
}

export interface InboxAdapterOptions {
  getPreviewUrl?: (file: InboxItemFileEntry) => string | undefined;
}

const PREVIEWABLE_IMAGE_EXTENSIONS = new Set([
  "jpg", "jpeg", "png", "gif", "webp", "bmp", "avif", "heic", "heif", "svg", "ico",
]);

export function isPreviewableImage(name: string): boolean {
  const separator = name.lastIndexOf(".");
  return separator >= 0 && PREVIEWABLE_IMAGE_EXTENSIONS.has(name.slice(separator + 1).toLowerCase());
}

export function fromInboxFiles(
  files: InboxItemFileEntry[],
  options: InboxAdapterOptions = {},
): FileBrowserItem[] {
  return files.map((file) => ({
    id: stableId("inbox", file.id),
    sourceId: file.id,
    fileId: file.transferFileId ?? undefined,
    name: file.name,
    relativePath: normalizeRelativePath(file.relativePath || file.name),
    size: file.size,
    localPath: file.localPath,
    previewUrl: file.missing || !isPreviewableImage(file.name)
      ? undefined
      : options.getPreviewUrl?.(file),
    status: file.missing ? "missing" : "completed",
    progress: file.missing ? 0 : 100,
  }));
}
