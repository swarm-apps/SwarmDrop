/**
 * ForeignFileAccess 实现：把 expo-file-system v56 的能力暴露给 Rust core。
 *
 * ## 这一层只做 Rust 做不到的事
 *
 * 接收侧的随机写**不在这里**。数据块由 Rust 直接写进应用私有的暂存区
 * （`mobile-core/src/file_staging.rs`），一次都不跨语言边界。本文件保留的是三件
 * 平台独占的能力：
 *
 * 1. 读发送源 —— 源 URI 不受我们控制。Android 上「选目录发送」走
 *    `Directory.pickDirectoryAsync()` → SAF tree，子项是 `content://`，只有
 *    expo-file-system 读得了。
 * 2. 发布到 SAF 目标 —— 只有 `ContentResolver` 能在用户选的公共目录里建 document。
 * 3. 删除 SAF 上已落地的文件。
 *
 * `file://` 目标的发布（建目录 + rename）同样在 Rust 侧，不经过这里。
 *
 * ## 为什么接收不再直接写 SAF
 *
 * `ContentResolver.openFileDescriptor` 拿到的 fd 不归本进程所有，它指向
 * `/storage/emulated/0` 的 FUSE 挂载。长时间大文件写入期间它会失效，而 `FileChannel`
 * 无从知晓：下一次 `lseek` 直接 `EBADF`，channel 自己却仍报告为 open。
 * 2026-08-07 实测 311 MB 接收稳定在 45 MB 处崩掉，换应用私有目录则完全正常。
 *
 * 所以外部位置只在发布时被**顺序**写一次（见 `copyIntoTarget`——它只用 read/write
 * 推进偏移，从不 `setOffset`）。
 */

import { Directory, File, FileMode } from "expo-file-system";
import {
  FfiError,
  type ForeignFileAccess,
  type MobileFileMetadata,
  type MobileFinalizedSink,
  type MobileSaveLocation,
} from "react-native-swarmdrop-core";

/**
 * 任意 JS error → `FfiError.Io` —— 必须包成 uniffi enum 形状，否则 uniffi 在
 * lift callback return 时认不出错误类型，会走 `handle_callback_unexpected_error`
 * 触发 Rust panic（catch_unwind 后 abort，日志只有 "Rust panic" 没有源信息）。
 */
async function wrapFfi<T>(
  operation: string,
  fn: () => Promise<T> | T,
): Promise<T> {
  try {
    return await fn();
  } catch (err) {
    throw FfiError.Io.new(`${operation}: ${errorDetail(err)}`);
  }
}

/**
 * 去掉 Java 栈帧，**保留 expo 的 `→ Caused by:` 链**。
 *
 * expo 的原生异常把整段 Java stacktrace 塞在 `message` 里，而这串会一路冒到 Rust、进传输
 * 失败事件、最后原样显示在 UI 的 toast 上（2026-08-07 截图里就是二十行 `at expo.modules...`）。
 *
 * ⚠️ 此前这里是 `.split("\n")[0]`——只取首行。栈是砍掉了，但 **`→ Caused by:` 那几行一起
 * 没了，而真实原因恰恰只在那里**。JSI 的首行永远是一句模板：
 *
 * ```
 * Call to function 'FileSystemFileHandle.writeBytes' has been rejected.
 * → Caused by: Unable to write to a file handle: '<真正的原因>'
 * ```
 *
 * 2026-08-09 排查一次 SAF 发布失败时，日志里就只剩那句模板——三次重试、三条一模一样的
 * 「has been rejected」，没有任何可据以判断的信息。按行过滤而不是按行数截断。
 */
function errorDetail(err: unknown): string {
  return (
    rawMessage(err)
      .split("\n")
      .map((line) => line.trim())
      // 栈帧才是噪音：`at expo.modules...` / `at com.facebook...`。
      .filter((line) => line.length > 0 && !line.startsWith("at "))
      .join(" ")
  );
}

function rawMessage(err: unknown): string {
  if (err instanceof Error) return `${err.name}: ${err.message}`;
  if (typeof err === "object" && err !== null && "message" in err) {
    return String(err.message);
  }
  return String(err);
}

/** 发布到 SAF 时的搬运块大小。大块少往返，同时留出让 UI 喘气的间隙。 */
const PUBLISH_CHUNK_BYTES = 4 * 1024 * 1024;

function isSafUri(uri: string): boolean {
  return uri.startsWith("content://");
}

export class ExpoFileAccess implements ForeignFileAccess {
  sourceMetadata(sourceId: string): Promise<MobileFileMetadata> {
    return wrapFfi("read source metadata", () => {
      const file = new File(sourceId);
      if (!file.exists) {
        throw new Error(`source does not exist: ${sourceId}`);
      }
      const name = decodeURIComponent(sourceId.split("/").pop() ?? sourceId);
      return {
        name,
        relativePath: name,
        size: BigInt(file.size ?? 0),
        modifiedAt: undefined,
        checksum: undefined,
        saveDir: undefined,
      };
    });
  }

  readSourceChunk(
    sourceId: string,
    offset: bigint,
    length: bigint,
  ): Promise<ArrayBuffer> {
    return wrapFfi(
      `read source chunk at offset ${offset} (${length} bytes)`,
      () => {
        // 读路径 source 是 expo-fs File / SAF content uri，统一 ReadOnly 模式打开。
        // source handle 不缓存——读取通常一次性 + RN core 端不会保持 sourceId 的
        // 并发引用，频繁 open/close 性能可以接受。
        const handle = new File(sourceId).open(FileMode.ReadOnly);
        try {
          handle.offset = Number(offset);
          // expo-fs readBytes 返回 Uint8Array；ubrn 期望 ArrayBuffer
          const bytes = handle.readBytes(Number(length));
          return bytes.buffer.slice(
            bytes.byteOffset,
            bytes.byteOffset + bytes.byteLength,
          ) as ArrayBuffer;
        } finally {
          handle.close();
        }
      },
    );
  }

  /**
   * 把 Rust 侧收齐的暂存文件发布到 SAF 目标目录。
   *
   * **只有 `content://` 目标会走到这里**——`file://` 目标由 Rust 直接 rename。
   *
   * 可重入：目标同名文件存在时复用它并整体覆盖（`FileMode.Truncate`），
   * 不生成 `foo (1).txt`。失败时删掉半成品，暂存仍在 Rust 侧、上层可重试。
   */
  publishToTarget(
    stagingUri: string,
    metadata: MobileFileMetadata,
  ): Promise<MobileFinalizedSink> {
    return wrapFfi("publish received file", async () => {
      const baseUri = saveLocationUri(metadata.saveDir);
      if (!isSafUri(baseUri)) {
        // Rust 侧只在 SAF 目标时才委托过来；走到这里说明分派逻辑漂了。
        throw new Error(`publish target is not a SAF tree: ${baseUri}`);
      }
      const { file, dir } = ensureSafTargetFile(baseUri, metadata.relativePath);
      try {
        await copyIntoTarget(stagingUri, file);
      } catch (err) {
        // 半成品必须删掉：暂存还在、上层会重试，留一个长度不足的文件在用户目录里
        // 只会误导（文件管理器里看着像收到了）。
        try {
          if (file.exists) file.delete();
        } catch {
          // best-effort
        }
        throw err;
      }
      // uri 必须是 createFile 实际返回的 document URI（系统可能改写重名），
      // dir 必须来自 Directory 对象——两者都推导不出来，见 core 的 finalize_sink 契约。
      return { uri: file.uri, dir };
    });
  }

  /**
   * 删除一个**已最终化**的文件。`uri` 是 publish 返回过的那个（`file://` 或 SAF
   * document URI），也就是落库到 `localPath` 的值。
   *
   * **文件已不存在不算错误**——删除幂等，重试路径上「删两次」很常见。
   *
   * 这里只回答「这个 URI 怎么删」这一层平台细节；「先删文件再删记录、失败不阻断」那套
   * 编排在 core 的 `inbox::delete_inbox_item`，三端共用。
   */
  deleteFinalizedFile(uri: string): Promise<void> {
    return wrapFfi("delete inbox file", () => {
      const file = new File(uri);
      if (file.exists) {
        file.delete();
      }
    });
  }
}

/**
 * 把暂存文件顺序搬进目标。
 *
 * **只用 `readBytes` / `writeBytes` 推进偏移，绝不 `setOffset`。** 这是刻意的：
 * SAF 的 fd 由外部 provider 持有，`lseek` 正是它失效时炸掉的那个调用
 * （`FileChannelImpl.position0` → `EBADF`）。顺序写把风险面压到最小。
 *
 * **不用 expo 的 `File.copy()`**，它的两条路径都不可用：
 * - copy 到具体文件（`isContainer=false`）会先 `deleteRecursively()` 再写，
 *   而 SAF document 删掉之后 uri 就失效了；
 * - copy 到目录（`isContainer=true`）会拿 **source 的文件名**建目标，
 *   而我们的暂存文件名是一串 hash。
 *
 * 每块之间让出一次事件循环：4 MiB 的同步 JSI 读写会阻塞 JS 线程几十毫秒，
 * 一个 300 MB 的文件连续搬完足以让界面卡死数秒。
 */
async function copyIntoTarget(stagingUri: string, target: File): Promise<void> {
  // `stagingUri` **带 `file://` scheme**——expo 的 `JavaFile` 走
  // `File(URI.create(uri))`，裸路径会抛 `URI is not absolute`。
  const sourceFile = new File(stagingUri);
  const totalBytes = sourceFile.size ?? 0;
  const source = sourceFile.open(FileMode.ReadOnly);
  let written = 0;
  try {
    const sink = target.open(FileMode.Truncate);
    try {
      for (;;) {
        const bytes = source.readBytes(PUBLISH_CHUNK_BYTES);
        if (bytes.byteLength === 0) break;
        sink.writeBytes(bytes);
        written += bytes.byteLength;
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
    } catch (err) {
      // **写到哪一步失败，本身就是判据。** 0 字节 = 一开就写不进（权限 / provider 拒绝）；
      // 写了一大半 = 空间不足或 fd 被 provider 回收。没有这个数字，几次重试的日志长得
      // 一模一样，什么也推不出来（2026-08-09 就卡在这里）。
      throw new Error(
        `${errorDetail(err)}（已写 ${written}/${totalBytes} 字节，块 ${PUBLISH_CHUNK_BYTES}）`,
      );
    } finally {
      sink.close();
    }
  } finally {
    source.close();
  }
}

/**
 * SAF tree URI 下按 `relativePath` 逐层建目录，返回叶子文件及其父目录 URI。
 *
 * SAF 不能拼路径 `new File(dir, "a/b/c.txt")`，要 `dir.createDirectory(name)`
 * 递归建子目录，叶子用 `dir.createFile(name, mime)`。
 *
 * `relativePath` 形如 `SwarmNote/sub/foo.txt`。
 */
function ensureSafTargetFile(
  baseUri: string,
  relativePath: string,
): { file: File; dir: string } {
  const segments = relativePath.split("/").filter(Boolean);
  if (segments.length === 0) {
    throw new Error(`SAF publish relativePath is empty: ${relativePath}`);
  }
  const fileName = segments[segments.length - 1];
  const dirSegments = segments.slice(0, -1);

  let currentDir = new Directory(baseUri);
  for (const seg of dirSegments) {
    const existing = findChildDirectory(currentDir, seg);
    currentDir = existing ?? currentDir.createDirectory(seg);
  }
  // 叶子目录的 SAF document URI —— 「打开文件夹」的真实容器目录事实源。
  // 不能由文件 document URI 字符串推导(docid 用 %2F 编码路径分隔符)。
  const dir = currentDir.uri;

  const existingFile = findChildFile(currentDir, fileName);
  if (existingFile) {
    // 已存在时一律复用，由 `FileMode.Truncate` 整体覆盖 —— **不要 delete + 重建**：
    // SAF 的异步 delete 没生效就被 createFile 命中 race，会生成 "foo (1).txt"
    // 或者返回不可写 fd（后续 writeBytes 报 "Bad file descriptor"）。
    // 覆盖同时也是 publish 可重入的实现方式：进程在搬运中途被杀之后，
    // 续传会重新发布一次并盖掉那个长度不足的产物。
    return { file: existingFile, dir };
  }
  // mimeType 必须传 "application/octet-stream"。看似 null 等价，但 expo-file-system
  // Android 端会把 null 兜底成 "text/plain"
  // (FileSystemDirectory.kt:79 `file.createFile(mimeType ?: "text/plain", fileName)`)。
  // 然后 DocumentsContract.createDocument(mimeType="text/plain", "foo.md") 会发现
  // "foo.md" 的扩展名跟 text/plain 不匹配，按 splitFileName 规则强制追加 ".txt"
  // (FileUtils#splitFileName)，导致 .md 落盘后变成 ".md.txt"。
  // application/octet-stream 是 MIME_TYPE_DEFAULT，splitFileName 对它特判
  // extFromMimeType=null，于是 displayName 原样保留 —— 这是 SAF 下「不要动我文件名」
  // 的标准约定。
  return {
    file: currentDir.createFile(fileName, "application/octet-stream"),
    dir,
  };
}

function findChildDirectory(parent: Directory, name: string): Directory | null {
  for (const entry of parent.list()) {
    if (entry instanceof Directory && entry.name === name) {
      return entry;
    }
  }
  return null;
}

function findChildFile(parent: Directory, name: string): File | null {
  for (const entry of parent.list()) {
    if (entry instanceof File && entry.name === name) {
      return entry;
    }
  }
  return null;
}

function saveLocationUri(saveDir: MobileSaveLocation | undefined): string {
  if (!saveDir) {
    throw new Error(
      "MobileFileMetadata.saveDir is missing: core did not provide the selected save directory",
    );
  }
  return saveDir.inner.path;
}
