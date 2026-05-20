import * as DocumentPicker from "expo-document-picker";
import { Directory, type File } from "expo-file-system";
import * as ImagePicker from "expo-image-picker";
import type { MobileTransferFile as TransferFile } from "react-native-swarmdrop-core";

/**
 * 通用文件选择 —— 通过系统 DocumentPicker 选任意类型文件。
 * 注意:v2 接线后字段从 `fileId/uri` 改为 `sourceId`,core 内部会分配 file_id。
 *
 * `copyToCacheDirectory: true` —— Android 上 SAF 返回的是 `content://...` URI,
 * expo-file-system 的 `File.open()` 不支持 content URI(会抛 "This method cannot
 * be used with content URIs"),进而把 Rust 拖到 panic。打开 copy 让 expo 帮我们
 * 拷贝成 `file://` URI;iOS 上同样需要拷贝才能稳定流式读取(NSItemProvider 临时
 * 授权过期问题)。
 */
export async function pickTransferFiles(): Promise<TransferFile[]> {
  const result = await DocumentPicker.getDocumentAsync({
    copyToCacheDirectory: true,
    multiple: true,
  });

  if (result.canceled) {
    return [];
  }

  return result.assets.map((asset) => ({
    sourceId: asset.uri,
    name: asset.name,
    relativePath: asset.name,
    size: BigInt(asset.size ?? 0),
  }));
}

export type MediaKind = "photos" | "videos" | "all";

/**
 * 相册选择 —— 通过 expo-image-picker 选照片/视频。
 * 默认 photos+videos,允许多选。
 */
export async function pickFromMediaLibrary(
  kind: MediaKind = "all",
): Promise<TransferFile[]> {
  const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
  if (!permission.granted) {
    throw new Error("Permission to access media library was denied");
  }

  const mediaTypes =
    kind === "photos"
      ? ["images" as const]
      : kind === "videos"
        ? ["videos" as const]
        : (["images", "videos"] as const);

  const result = await ImagePicker.launchImageLibraryAsync({
    mediaTypes: [...mediaTypes],
    allowsMultipleSelection: true,
    selectionLimit: 0,
    quality: 1,
    exif: false,
    base64: false,
  });

  if (result.canceled) {
    return [];
  }

  return result.assets.map((asset, idx) => {
    const fallbackExt =
      asset.type === "video"
        ? ".mp4"
        : asset.mimeType?.includes("png")
          ? ".png"
          : ".jpg";
    const baseName =
      asset.fileName ?? `${asset.type ?? "media"}-${idx + 1}${fallbackExt}`;
    return {
      sourceId: asset.uri,
      name: baseName,
      relativePath: baseName,
      size: BigInt(asset.fileSize ?? 0),
    };
  });
}

/**
 * 目录选择 —— 弹起系统目录选择器，递归扫描所有文件。
 *
 * 用 expo-file-system 55+ 的 `Directory.pickDirectoryAsync()`，iOS / Android
 * 双平台都通过它统一处理（iOS DocumentPicker folder 模式 + Android SAF
 * StorageAccessFramework，由 expo 内部桥接）。
 *
 * 返回的 TransferFile 数组里：
 * - `relativePath` 形如 `<rootDirName>/<sub>/<file.ext>`，保留目录结构
 * - `sourceId` 用 file:// URI（Rust core 端 ForeignFileAccess 用此打开源文件）
 *
 * 注意：core 层 prepareSend 已支持任意 `relativePath`（包含 "/" 的视为嵌套），
 * RN 这里只负责拿到正确的相对路径串。文件 I/O 走 RN 的 ForeignFileAccess
 * adapter，不在此函数中读取实际内容。
 */
export async function pickTransferDirectory(): Promise<TransferFile[]> {
  const root = await Directory.pickDirectoryAsync();
  const rootName = root.name || "directory";

  const files: TransferFile[] = [];

  // 迭代式 DFS（递归在 RN JS 引擎栈深度有限制，且目录可能很深）
  const stack: { dir: Directory; prefix: string }[] = [
    { dir: root, prefix: rootName },
  ];

  while (stack.length > 0) {
    const top = stack.pop();
    if (!top) break;
    const { dir, prefix } = top;
    let entries: (Directory | File)[];
    try {
      entries = dir.list();
    } catch (err) {
      console.warn(
        "[pickTransferDirectory] list failed:",
        dir.uri,
        err instanceof Error ? err.message : err,
      );
      continue;
    }
    for (const entry of entries) {
      const entryName = entry.name;
      // 跳过常见无意义条目（系统元信息）
      if (entryName === ".DS_Store" || entryName === "Thumbs.db") continue;
      if (entry instanceof Directory) {
        stack.push({ dir: entry, prefix: `${prefix}/${entryName}` });
      } else {
        files.push({
          sourceId: entry.uri,
          name: entryName,
          relativePath: `${prefix}/${entryName}`,
          size: BigInt(entry.size ?? 0),
        });
      }
    }
  }

  return files;
}
