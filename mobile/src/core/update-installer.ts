/**
 * Android APK installer.
 *
 * 把 APK 下载到 cache 目录，然后用 IntentLauncher 把 content:// URI 交给系统
 * PackageInstaller。系统仍会弹"安装新版本？"确认框 —— Android 不允许第三方
 * 应用静默安装。iOS 由 TestFlight / App Store 处理，本模块直接抛错。
 */

// `createDownloadResumable` / `getContentUriAsync` 在 expo-file-system v18+
// 仅保留在 legacy 命名空间。新的 OOP File API 还没暴露进度回调和 content://
// 帮手，所以这里继续用 legacy 导入。
import * as FileSystem from "expo-file-system/legacy";
import * as IntentLauncher from "expo-intent-launcher";
import { Platform } from "react-native";

const FLAG_GRANT_READ_URI_PERMISSION = 0x00000001;
const FLAG_ACTIVITY_NEW_TASK = 0x10000000;

export interface DownloadProgress {
  downloaded: number;
  total: number;
  percent: number;
}

export interface DownloadHandle {
  cancel(): Promise<void>;
}

export class UpdateNotSupportedOnIosError extends Error {
  constructor() {
    super("In-app updates are not supported on iOS");
    this.name = "UpdateNotSupportedOnIosError";
  }
}

function apkPath(): string {
  const cacheDir = FileSystem.cacheDirectory;
  if (!cacheDir) throw new Error("FileSystem.cacheDirectory unavailable");
  return `${cacheDir}swarmdrop-update.apk`;
}

/**
 * 下载 APK 并触发系统安装器，resolve 后控制权已经交给系统对话框；如果用户
 * 取消，下次 checkForUpdate 会再次弹出 prompt。
 */
export async function downloadAndInstallApk(
  url: string,
  onProgress?: (p: DownloadProgress) => void,
): Promise<DownloadHandle> {
  if (Platform.OS !== "android") {
    throw new UpdateNotSupportedOnIosError();
  }

  const target = apkPath();

  // 清掉上次残留的 partial 文件，避免 resume 冲突
  const info = await FileSystem.getInfoAsync(target);
  if (info.exists) {
    await FileSystem.deleteAsync(target, { idempotent: true });
  }

  const resumable = FileSystem.createDownloadResumable(url, target, {}, (p) => {
    if (!onProgress) return;
    const total = p.totalBytesExpectedToWrite;
    const downloaded = p.totalBytesWritten;
    const percent = total > 0 ? Math.round((downloaded / total) * 100) : 0;
    onProgress({ downloaded, total, percent });
  });

  const handle: DownloadHandle = {
    async cancel() {
      try {
        await resumable.cancelAsync();
      } catch {
        // ignore — 下载可能已结束
      }
    },
  };

  const result = await resumable.downloadAsync();
  if (!result?.uri) {
    throw new Error("Download produced no file");
  }

  const contentUri = await FileSystem.getContentUriAsync(result.uri);

  await IntentLauncher.startActivityAsync("android.intent.action.VIEW", {
    data: contentUri,
    type: "application/vnd.android.package-archive",
    flags: FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK,
  });

  return handle;
}
