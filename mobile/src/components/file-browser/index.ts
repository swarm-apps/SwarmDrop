export {
  inboxFileId,
  isPathInsideDirectory,
  normalizeDirectoryPath,
  normalizeRelativePath,
  removeSelectedDirectory,
  selectedFileId,
  sessionFileId,
} from "@/core/file-browser-identity";
export {
  fromInboxFiles,
  fromOfferFiles,
  fromProjection,
  fromSelectedFiles,
} from "./adapters";
export { FileBrowser } from "./file-browser";
export {
  buildFileBrowserTree,
  flattenVisibleNodes,
} from "./tree-data";
export type {
  FileBrowserActions,
  FileBrowserItem,
  FileBrowserListContext,
  FileBrowserProps,
  FileBrowserScope,
  FileBrowserStatus,
  FileBrowserView,
} from "./types";
