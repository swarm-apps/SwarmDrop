import type { MobileTransferFile as TransferFile } from "react-native-swarmdrop-core";
import { create } from "zustand";

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
  removeSharedByPath: (relativePath: string) => void;
  clearSharedFiles: () => void;
}

export const useShareStore = create<ShareState>((set) => ({
  sharedFiles: [],
  setSharedFiles: (files) => set({ sharedFiles: files }),
  removeSharedByPath: (relativePath) =>
    set((s) => ({
      sharedFiles: s.sharedFiles.filter((f) => f.relativePath !== relativePath),
    })),
  clearSharedFiles: () => set({ sharedFiles: [] }),
}));
