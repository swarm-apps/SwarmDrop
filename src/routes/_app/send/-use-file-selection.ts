/**
 * useFileSelection
 * 文件选择状态管理 Hook — 管理 ScannedFile 列表，
 * 返回统一 FileBrowser 所需的 items / 统计数据
 *
 * 流程：pickFiles → scanSources（后端扫描）→ 展示文件树 → prepareSend
 */

import { useCallback, useMemo, useState } from "react";
import { fromEnumeratedFiles, type FileBrowserItem } from "@/components/file-browser";
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
  /** 添加文件来源（文件或文件夹），后端扫描后加入列表 */
  addSources: (sources: FileSource[]) => Promise<void>;
  /** 移除文件（按 relativePath 匹配） */
  removeFile: (relativePath: string) => void;
  /** 清空所有 */
  clear: () => void;
  /** 获取所有扫描到的文件列表（用于 prepareSend） */
  getScannedFiles: () => ScannedFile[];
}

export function useFileSelection(): FileSelection {
  const [files, setFiles] = useState<ScannedFile[]>([]);

  const items = useMemo(() => fromEnumeratedFiles(files), [files]);

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

    const results = await commands.scanSources(sources);

    const newFiles = results.flatMap((result) => result.files);

    if (newFiles.length > 0) {
      setFiles((prev) => [...prev, ...newFiles]);
    }
  }, []);

  const removeFile = useCallback((relativePath: string) => {
    setFiles((prev) => {
      // 匹配精确路径或目录前缀（移除目录下所有文件）
      const dirPrefix = relativePath.endsWith("/")
        ? relativePath
        : `${relativePath}/`;
      return prev.filter(
        (f) =>
          f.relativePath !== relativePath &&
          !f.relativePath.startsWith(dirPrefix),
      );
    });
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
    addSources,
    removeFile,
    clear,
    getScannedFiles,
  };
}
