import * as DocumentPicker from "expo-document-picker";
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
