/**
 * 用系统能力打开/预览一个已接收的本地文件 —— 收件箱「打开」主动作的跨平台管道。
 *
 * - iOS:QuickLook(QLPreviewController,经 react-native-file-viewer),图片/视频/
 *   PDF/Office 全覆盖;需要**解码后的绝对路径**(中文文件名在 file:// URI 里是
 *   percent-encoded 的,直接传会找不到文件)。
 * - Android:ACTION_VIEW 交给系统应用。SAF content:// 直接用;私有目录 file://
 *   经 Expo FileProvider 换成 content://(其他应用无权直读本 app 私有目录)。
 *   不显式 setType —— resolver 会向 provider 查 MIME 完成匹配,避免 type+data
 *   同设的兼容坑。
 *
 * 无处理应用/预览失败会抛错,降级(分享面板/toast)由调用方编排。
 */

import { getContentUriAsync } from "expo-file-system/legacy";
import { Platform } from "react-native";
import FileViewer from "react-native-file-viewer";
import { startViewIntent } from "@/core/saf-intent";

export async function openFileWithSystem(localPath: string): Promise<void> {
  if (Platform.OS === "android") {
    const contentUri = localPath.startsWith("content://")
      ? localPath
      : await getContentUriAsync(localPath);
    await startViewIntent({ data: contentUri });
    return;
  }
  await FileViewer.open(
    decodeURIComponent(localPath.replace(/^file:\/\//, "")),
  );
}
