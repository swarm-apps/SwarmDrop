import * as DocumentPicker from "expo-document-picker";
import type { TransferFile } from "react-native-swarmdrop-core";

export async function pickTransferFiles(): Promise<TransferFile[]> {
  const result = await DocumentPicker.getDocumentAsync({
    copyToCacheDirectory: false,
    multiple: true,
  });

  if (result.canceled) {
    return [];
  }

  return result.assets.map((asset, index) => ({
    fileId: stableFileId(asset.uri, index),
    name: asset.name,
    relativePath: null,
    uri: asset.uri,
    size: asset.size ?? 0,
    isDirectory: false,
  }));
}

function stableFileId(uri: string, index: number): string {
  let hash = 0;
  for (let i = 0; i < uri.length; i += 1) {
    hash = (hash * 31 + uri.charCodeAt(i)) | 0;
  }
  return `file-${index}-${Math.abs(hash).toString(36)}`;
}
