import type { MobileTransferFile as TransferFile } from "react-native-swarmdrop-core";

/**
 * expo-share-intent 交付的单个分享文件的最小形状(只取我们要的字段,不 import 其类型,
 * 与库解耦)。`path` 是库已拷成 App 拥有的 `file://` 路径(权限稳定,可撑长传)。
 */
export interface SharedFileLike {
  path?: string;
  fileName?: string;
  size?: number | null;
  mimeType?: string;
}

/**
 * 分享文件 → 现有发送管线的 `TransferFile`。
 *
 * - `sourceId` 用库给的 `file://` path(等于 DocumentPicker copyToCacheDirectory 后的形态,
 *   `ForeignFileAccess.readSourceChunk` 能直接读)。
 * - 分享无目录结构,`relativePath` 用平铺文件名。
 * - 支持多文件(ACTION_SEND_MULTIPLE):整段数组映射。
 * - 无 path 的项(异常/未拷成功)跳过。
 */
export function shareFilesToTransferFiles(
  files: readonly SharedFileLike[] | null | undefined,
): TransferFile[] {
  if (!files) return [];
  const out: TransferFile[] = [];
  for (let i = 0; i < files.length; i++) {
    const f = files[i];
    if (!f?.path) continue;
    const name =
      f.fileName?.trim() ||
      decodeURIComponent(f.path.split("/").pop() || `file-${i + 1}`);
    out.push({
      sourceId: f.path,
      name,
      relativePath: name,
      size: BigInt(f.size ?? 0),
    });
  }
  return out;
}
