/**
 * Android Intent 启动工具 —— 共享给 update-installer / 传输详情等多处使用。
 *
 * 把常用的 ACTION_VIEW + 标记位封装成一个函数；iOS / 其它 scheme 由调用方处理。
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
 * 用系统能力打开传输保存目录。
 *
 * - Android `content://` (SAF tree)：ACTION_VIEW + `vnd.android.document/directory`
 *   交给 DocumentsUI / 默认 Files app
 * - file:// / iOS：走 `Linking.openURL`；iOS 上 file:// 往往不被系统接管，
 *   失败由调用方降级到「复制路径到剪贴板」
 */
export async function openSafTreeUri(uri: string): Promise<void> {
  if (Platform.OS === "android" && uri.startsWith("content://")) {
    await startViewIntent({
      data: uri,
      type: "vnd.android.document/directory",
    });
    return;
  }
  const supported = await Linking.canOpenURL(uri);
  if (!supported) {
    throw new Error(`Unsupported URI scheme: ${uri}`);
  }
  await Linking.openURL(uri);
}
