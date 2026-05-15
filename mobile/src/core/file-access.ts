import * as DocumentPicker from "expo-document-picker";
import type { MobileTransferFile as TransferFile } from "react-native-swarmdrop-core";

/**
 * 选择待传输的文件 —— 供 prepare_send 使用。
 * 注意：v2 接线后字段从 `fileId/uri` 改为 `sourceId`，core 内部会分配 file_id。
 */
export async function pickTransferFiles(): Promise<TransferFile[]> {
  const result = await DocumentPicker.getDocumentAsync({
    copyToCacheDirectory: false,
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
