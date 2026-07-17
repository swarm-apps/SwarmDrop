import * as FileSystem from "expo-file-system/legacy";
import * as IntentLauncher from "expo-intent-launcher";
import { Platform } from "react-native";
import type { ApkInstaller } from "./ports";

/** android.content.Intent#FLAG_GRANT_READ_URI_PERMISSION */
const FLAG_GRANT_READ_URI_PERMISSION = 0x00000001;
/** android.content.Intent#FLAG_ACTIVITY_NEW_TASK */
const FLAG_ACTIVITY_NEW_TASK = 0x10000000;

/** iOS / 非 Android 平台不支持 in-app 安装(由 TestFlight / App Store 接管)。 */
export class ApkInstallNotSupportedOnIosError extends Error {
  constructor() {
    super("In-app APK install is not supported on iOS");
    this.name = "ApkInstallNotSupportedOnIosError";
  }
}

/**
 * 创建方案 A 的 ApkInstaller。install(apkPath):
 *   file:// 路径 → getContentUriAsync → ACTION_VIEW(package-archive)→ 系统 PackageInstaller。
 * intent 派发即 resolve(fire-and-forget);非 Android 抛 ApkInstallNotSupportedOnIosError。
 */
export function createExpoApkInstaller(): ApkInstaller {
  return {
    async install(apkPath: string): Promise<void> {
      if (Platform.OS !== "android") {
        throw new ApkInstallNotSupportedOnIosError();
      }
      // expo-file-system 自带 FileProvider:把本地 file:// 转成可对外授权的 content:// URI。
      const contentUri = await FileSystem.getContentUriAsync(apkPath);
      await IntentLauncher.startActivityAsync("android.intent.action.VIEW", {
        data: contentUri,
        type: "application/vnd.android.package-archive",
        flags: FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK,
      });
      // 不等安装结果:控制权已交给系统对话框(fire-and-forget handoff)。
    },
  };
}
