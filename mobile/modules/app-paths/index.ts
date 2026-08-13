import { requireOptionalNativeModule } from "expo-modules-core";
import { Platform } from "react-native";

interface AppPathsNativeModule {
  applicationSupportDirectory(): string;
}

/** iOS-only；未编入原生包时为 null，调用方给出可诊断的错误而非静默降级。 */
const AppPaths = requireOptionalNativeModule<AppPathsNativeModule>("AppPaths");

/**
 * iOS 的 Application Support 目录（`file://` URI，目录保证已创建）。
 *
 * 这是应用**私有数据区**——数据库与接收暂存的家。它与用户可见的 `Documents`
 * 分开：后者在开启文件共享后整个暴露给「文件」App，内部数据不能留在那里。
 *
 * **仅 iOS**。Android 的私有数据区就是 `Paths.document.uri`（系统本就不对用户暴露
 * 它），不需要也不应该走这条路径——误调会抛错，而不是返回一个看似能用的错误路径。
 */
export function applicationSupportDirectory(): string {
  if (Platform.OS !== "ios") {
    throw new Error(
      `applicationSupportDirectory() is iOS-only (called on ${Platform.OS}); ` +
        "use getPrivateDataDir() from @/core/paths instead",
    );
  }
  if (!AppPaths) {
    throw new Error(
      "AppPaths native module is not linked; run `pnpm prebuild` and rebuild the iOS app",
    );
  }
  return AppPaths.applicationSupportDirectory();
}
