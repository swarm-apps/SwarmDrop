import type { MobileTransferFile as TransferFile } from "react-native-swarmdrop-core";
import { create } from "zustand";
import {
  removeSelectedDirectory as filterSelectedDirectory,
  removeSelectedFile,
} from "@/core/file-browser-identity";

/**
 * 入站分享的「在途文件」——从系统分享(burnt 之外的 expo-share-intent)接到、待发送的文件。
 *
 * 刻意**不持久化**：v1 不做「未引导时暂存分享、引导后恢复」。根布局的 ShareIntentHandler
 * 收到分享 → 映射成 TransferFile[] → set 进来 → push `/send/share-target`;发送成功或取消
 * 后 clear。与交互式发送页的 `mobile-core-store.selectedFiles` 分开,互不污染。
 */
interface ShareState {
  sharedFiles: TransferFile[];
  setSharedFiles: (files: TransferFile[]) => void;
  removeSharedBySourceId: (sourceId: string) => void;
  removeSharedDirectory: (relativeDirectory: string) => void;
  clearSharedFiles: () => void;
}

export const useShareStore = create<ShareState>((set) => ({
  sharedFiles: [],
  setSharedFiles: (files) => set({ sharedFiles: files }),
  removeSharedBySourceId: (sourceId) =>
    set((s) => ({
      sharedFiles: removeSelectedFile(s.sharedFiles, sourceId),
    })),
  removeSharedDirectory: (relativeDirectory) =>
    set((s) => ({
      sharedFiles: filterSelectedDirectory(s.sharedFiles, relativeDirectory),
    })),
  clearSharedFiles: () => set({ sharedFiles: [] }),
}));
