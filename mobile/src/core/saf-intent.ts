/**
 * 打开系统文件管理器 / Android Intent 启动工具 —— 共享给 update-installer /
 * 传输详情 / 收件箱详情等多处使用。
 *
 * `startViewIntent` 封装 ACTION_VIEW + 标记位(Android-only);
 * `openSaveFolder` 是跨平台的「打开保存目录」:Android 走 SAF intent,iOS 走
 * shareddocuments:// 唤起系统「文件」App,失败统一抛错由调用方降级提示。
 */

import * as IntentLauncher from "expo-intent-launcher";
import { Linking, Platform } from "react-native";

/** android.content.Intent#FLAG_GRANT_READ_URI_PERMISSION */
export const FLAG_GRANT_READ_URI_PERMISSION = 0x00000001;
/** android.content.Intent#FLAG_ACTIVITY_NEW_TASK */
export const FLAG_ACTIVITY_NEW_TASK = 0x10000000;

interface ViewIntentParams {
  data: string;
  /** 显式 MIME type；不传走 Android 自动推断 */
  type?: string;
  /** 默认 FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK */
  flags?: number;
}

/**
 * Android `Intent.ACTION_VIEW`，data 通常是 content:// 或 file://；
 * 非 Android 平台抛错，调用方应自行降级。
 */
export function startViewIntent(
  params: ViewIntentParams,
): Promise<IntentLauncher.IntentLauncherResult> {
  if (Platform.OS !== "android") {
    throw new Error("startViewIntent is android-only");
  }
  const { data, type, flags } = params;
  return IntentLauncher.startActivityAsync("android.intent.action.VIEW", {
    data,
    type,
    flags: flags ?? FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK,
  });
}

/**
 * 平台是否有系统入口能打开该保存目录 —— UI 用它决定「打开文件夹」入口是否渲染,
 * 别让用户点一个注定失败的按钮:
 * - iOS:file:// 沙盒目录可经 shareddocuments:// 在「文件」App 打开 → true
 * - Android content://(SAF,用户自定义目录):DocumentsUI 可 ACTION_VIEW → true
 * - Android file://(app 私有目录,默认接收位置):系统没有文件管理器能浏览
 *   私有目录(除非自建 DocumentsProvider) → false
 */
export function canOpenSaveFolder(uri: string): boolean {
  if (Platform.OS === "ios") return true;
  return uri.startsWith("content://");
}

/**
 * 用系统文件管理器打开保存目录。
 *
 * - Android `content://` (SAF tree)：ACTION_VIEW + `vnd.android.document/directory`
 *   交给 DocumentsUI / 默认 Files app
 * - iOS `file://`：换成 `shareddocuments://` scheme 让系统「文件」App 就地打开
 *   (不经 canOpenURL —— 未在 LSApplicationQueriesSchemes 声明的 scheme 会误报 false)
 * - 其余走 `Linking.openURL`；失败由调用方降级提示
 */
export async function openSaveFolder(uri: string): Promise<void> {
  if (Platform.OS === "android" && uri.startsWith("content://")) {
    await startViewIntent({
      data: uri,
      type: "vnd.android.document/directory",
    });
    return;
  }
  if (Platform.OS === "ios" && uri.startsWith("file://")) {
    await Linking.openURL(uri.replace(/^file:\/\//, "shareddocuments://"));
    return;
  }
  const supported = await Linking.canOpenURL(uri);
  if (!supported) {
    throw new Error(`Unsupported URI scheme: ${uri}`);
  }
  await Linking.openURL(uri);
}
