import { Paths } from "expo-file-system";
import { Platform } from "react-native";
import { applicationSupportDirectory } from "../../modules/app-paths";

/**
 * 应用**私有数据区**——数据库与接收暂存（`<data_dir>/staging/`）的家。
 *
 * 它与用户可见的**接收区**（`@/core/receive-location`）是两个角色，不能合用一个目录：
 * 二者的可见性、备份策略、以及「用户删掉它意味着什么」全都不同。放在一起时，任何一方的
 * 可见性决策都会强加给另一方——iOS 正是因此长期无法开启文件共享（一开就连库带暂存一起
 * 暴露给「文件」App，用户可以删）。
 *
 * - **iOS** — `Library/Application Support/`。不能用 `Paths.document.uri`：`Documents`
 *   已随 `UIFileSharingEnabled` 整个对用户可见。
 * - **Android** — `Paths.document.uri`（即 `<internal>/files/`）。系统本就不对用户暴露
 *   应用内部存储，无需另辟位置。
 */
export function getPrivateDataDir(): string {
  return Platform.OS === "ios"
    ? applicationSupportDirectory()
    : Paths.document.uri;
}
