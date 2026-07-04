import { requireOptionalNativeModule } from "expo-modules-core";

interface ContentShareNativeModule {
  shareContentUri(
    uri: string,
    fileName: string,
    dialogTitle?: string,
  ): Promise<void>;
}

/** Android-only；旧原生包（未编入本模块）里为 null，调用方需给出可诊断的错误。 */
const ContentShare =
  requireOptionalNativeModule<ContentShareNativeModule>("ContentShare");

/**
 * 系统分享面板分享一个 content:// 文档，零拷贝（不经 cache 副本）。
 * `fileName` 用于推断 MIME（document URI 里取不出可读文件名）。
 */
export async function shareContentUri(
  uri: string,
  fileName: string,
  dialogTitle?: string,
): Promise<void> {
  if (!ContentShare) {
    throw new Error("ContentShare native module is not linked");
  }
  await ContentShare.shareContentUri(uri, fileName, dialogTitle);
}
