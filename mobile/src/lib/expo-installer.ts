import * as FileSystem from "expo-file-system/legacy";
import * as IntentLauncher from "expo-intent-launcher";
import { AppState, Platform } from "react-native";
import type { ApkInstallBlockReason, ApkInstaller } from "./ports";

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
 *   前台门禁 → file:// 路径 → getContentUriAsync →
 *   ACTION_VIEW(package-archive)→ 系统 PackageInstaller。
 * intent 派发即 resolve(fire-and-forget);非 Android 抛 ApkInstallNotSupportedOnIosError。
 *
 * **前台门禁是本文件存在的主要理由。** Android 10+ 禁止后台启动 Activity:app 不在前台时
 * 派发的安装 intent 会被系统**静默丢弃**(不抛异常、不回调,只在 logcat 留一行
 * `Background activity launch blocked!`),而 `startActivityAsync` 照常 resolve。于是 UI
 * 显示「等待系统确认」而系统那边什么都没发生 —— 这正是 SwarmDrop v0.12.3 熄屏更新时
 * 用户被卡死的原因。宁可不发并回一个可判别的 reason,让调用方留在 ready 稍后重试。
 *
 * ⚠️ **本文件由 `@swarmhive-rn` registry 分发,上游在 SwarmHive
 * `packages/registry-rn/registry/rn/lib/expo-installer.ts`。要改请改上游再重新拉取。**
 */
export function createExpoApkInstaller(): ApkInstaller {
  return {
    async install(
      apkPath: string,
    ): Promise<void | { reason: ApkInstallBlockReason }> {
      if (Platform.OS !== "android") {
        throw new ApkInstallNotSupportedOnIosError();
      }
      // 返回而非抛错:门禁挡下不是失败,什么都没发生过、产物完好。抛错会让 engine 进 error
      // 并广播一个假故障,逼每个订阅者去认识这个平台错误类再把状态推回来。
      if (AppState.currentState !== "active") {
        return { reason: "background" };
      }

      // expo-file-system 自带 FileProvider:把本地 file:// 转成可对外授权的 content:// URI。
      const contentUri = await FileSystem.getContentUriAsync(apkPath);
      await IntentLauncher.startActivityAsync("android.intent.action.VIEW", {
        data: contentUri,
        type: "application/vnd.android.package-archive",
        flags: FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK,
      });
      // 不等安装结果:控制权已交给系统对话框(fire-and-forget handoff)。
      //
      // **未授权「安装未知应用」时也照发**:Android 自己会把用户领到授权页,授权后返回,
      // engine 仍在 ready,点「立即安装」即可。做一个只能靠猜的权限探测反而会挡掉本来
      // 能装的更新,见 ports.ts 的 ApkInstallBlockReason。
    },
  };
}
