/**
 * useFileSelection
 * 文件选择状态管理 Hook — 管理 ScannedFile 列表，
 * 返回统一 FileBrowser 所需的 items / 统计数据
 *
 * 流程：pickFiles → scanSources（后端扫描）→ 展示文件树 → prepareSend
 */

import { useCallback, useMemo, useState } from "react";
import { isPathInsideDirectory, type FileBrowserItem } from "@swarmdrop/shared-view";
import type { FileBrowserTarget } from "@swarmdrop/file-browser";
import { itemsFromScannedFiles } from "@/lib/file-browser-adapters";
import type { FileSource } from "@/lib/bindings";
import type { ScannedFile } from "@/lib/types";
import { commands } from "@/lib/bindings";

export interface FileSelection {
  /** 统一文件浏览模型 */
  items: FileBrowserItem[];
  /** 文件总数 */
  totalCount: number;
  /** 总大小 */
  totalSize: number;
  /** 是否有文件 */
  hasFiles: boolean;
  /**
   * 是否正在后端扫描来源（枚举目录、取 size）。
   *
   * 这一段此前**零反馈**：没有 spinner、没有骨架、按钮不变灰，选完一个大目录后界面
   * 就是纯粹的无响应。它才是真正的「选完文件之后」——校验和计算要到点了发送才开始。
   */
  isScanning: boolean;
  /** 添加文件来源（文件或文件夹），后端扫描后加入列表 */
  addSources: (sources: FileSource[]) => Promise<void>;
  /** 移除文件或整个目录。直接接 `FileBrowser` 的 `onRemove`。 */
  removeTarget: (target: FileBrowserTarget) => void;
  /** 清空所有 */
  clear: () => void;
  /** 获取所有扫描到的文件列表（用于 prepareSend） */
  getScannedFiles: () => ScannedFile[];
}

export function useFileSelection(): FileSelection {
  const [files, setFiles] = useState<ScannedFile[]>([]);
  // 计数而非布尔：并发 addSources（连续拖入两批）下，先返回的那批不能把标记清掉。
  const [scanCount, setScanCount] = useState(0);

  const items = useMemo(() => itemsFromScannedFiles(files), [files]);

  // 统计
  const stats = useMemo(() => {
    let totalSize = 0;
    for (const f of files) {
      totalSize += f.size;
    }
    return { totalSize, totalCount: files.length };
  }, [files]);

  const addSources = useCallback(async (sources: FileSource[]) => {
    if (sources.length === 0) return;

    setScanCount((n) => n + 1);
    try {
      const results = await commands.scanSources(sources);

      const newFiles = results.flatMap((result) => result.files);

      if (newFiles.length > 0) {
        setFiles((prev) => [...prev, ...newFiles]);
      }
    } finally {
      setScanCount((n) => n - 1);
    }
  }, []);

  /**
   * 判据走共享的 `isPathInsideDirectory`（它同时覆盖「路径本身」与「目录下的所有文件」），
   * 而不是手写前缀匹配——那份手写版还漏了路径归一，Windows 的 `a\\b\\c.txt` 与 items 侧
   * 归一过的路径对不上。另两端同一个操作也都用它。
   */
  const removeTarget = useCallback((target: FileBrowserTarget) => {
    const path =
      target.type === "directory" ? target.relativePath : target.item.relativePath;
    setFiles((prev) => prev.filter((f) => !isPathInsideDirectory(f.relativePath, path)));
  }, []);

  const clear = useCallback(() => {
    setFiles([]);
  }, []);

  const getScannedFiles = useCallback((): ScannedFile[] => {
    return files;
  }, [files]);

  return {
    items,
    totalCount: stats.totalCount,
    totalSize: stats.totalSize,
    hasFiles: stats.totalCount > 0,
    isScanning: scanCount > 0,
    addSources,
    removeTarget,
    clear,
    getScannedFiles,
  };
}
