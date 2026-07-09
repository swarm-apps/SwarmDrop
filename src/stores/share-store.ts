/**
 * Share Store
 *
 * 入站分享的「在途来源」——从系统「用 SwarmDrop 打开」接到、待发送的文件/文件夹路径
 * （已包装成 FileSource）。
 *
 * 刻意**不持久化**：v1 不做「未引导时暂存、引导后恢复」。根级 ExternalOpenHandler
 * 收到 external-file-open → 包装成 FileSource[] → setSources → navigate
 * `/send/share-target`；选设备屏 mount 时 `consume()` 一次性取走并清空，与交互式发送页
 * （useFileSelection 内部 state）互不污染。对标 SwarmDrop-RN 的 src/stores/share-store.ts。
 */

import { create } from "zustand";
import type { FileSource } from "@/lib/bindings";

interface ShareState {
  /** 待 share-target 屏消费的在途来源（外部打开的本地路径）。 */
  sources: FileSource[];
  /** 预选目标设备（「重新发送」携带原目标；外部打开不设置）。 */
  presetPeerId: string | null;
  setSources: (sources: FileSource[], presetPeerId?: string) => void;
  /** 原子取用：返回当前来源并清空。「读一次即丢弃」的语义收在这里，消费方无需自己 clear。 */
  consume: () => { sources: FileSource[]; presetPeerId: string | null };
}

export const useShareStore = create<ShareState>((set, get) => ({
  sources: [],
  presetPeerId: null,
  setSources: (sources, presetPeerId) =>
    set({ sources, presetPeerId: presetPeerId ?? null }),
  consume: () => {
    const { sources, presetPeerId } = get();
    if (sources.length > 0) set({ sources: [], presetPeerId: null });
    return { sources, presetPeerId };
  },
}));
