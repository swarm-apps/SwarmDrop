import { Directory, File, Paths } from "expo-file-system";
import { Platform } from "react-native";

/**
 * iOS 上曾经住在 `Documents` 里的应用内部数据。
 *
 * 这些名字是 mobile-core 在 `data_dir` 下创建的东西（`swarmdrop.db` 及其 WAL/SHM 边车、
 * `staging/` 暂存区、`device_config.json`），外加旧的接收子目录 `transfers/`。
 */
const LEGACY_FILES = [
  "swarmdrop.db",
  "swarmdrop.db-wal",
  "swarmdrop.db-shm",
  "device_config.json",
] as const;

const LEGACY_DIRECTORIES = ["staging", "transfers"] as const;

/**
 * 清掉 iOS `Documents` 里的历史内部数据。
 *
 * **为什么必须做，尽管这次明确「不迁移」。** 不迁移说的是数据不搬——历史与收件箱丢掉是
 * 接受的代价。但同一次改动把 `UIFileSharingEnabled` 打开了：`Documents` 从此整个暴露在
 * 「文件」App 里。留着不管的后果不是「多占点空间」，而是**用户会看到自己的数据库文件和
 * 一堆 hash 命名的暂存半成品，并且可以删**——那正是 `DESIGN.md` 的
 * 「App-private data never shares a directory with the receive area」要杜绝的画面。
 * 不清理的话，这条不变量只对全新安装成立。
 *
 * 也顺带解决孤儿 staging：新的暂存区在 Application Support 下，旧的那份再也不会被
 * 任何一次续传认领，只会一直占着空间。
 *
 * 幂等且**绝不抛错**：清理失败不该挡住启动——最坏情况只是用户在「文件」App 里多看见
 * 几个文件，而为此让整个应用起不来显然更糟。
 */
export function cleanupLegacyIosDocuments(): void {
  // Android 的 `Paths.document.uri` 现在**仍然是**私有数据区，里面的东西正在被使用。
  if (Platform.OS !== "ios") return;

  for (const name of LEGACY_FILES) {
    try {
      const file = new File(Paths.document, name);
      if (file.exists) file.delete();
    } catch {
      // 单个失败不影响其余：逐个 try 而不是整体包一层。
    }
  }

  for (const name of LEGACY_DIRECTORIES) {
    try {
      const directory = new Directory(Paths.document, name);
      if (directory.exists) directory.delete();
    } catch {
      // 同上。
    }
  }
}
