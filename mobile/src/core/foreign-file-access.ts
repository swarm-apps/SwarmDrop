/**
 * ForeignFileAccess 实现：把 expo-file-system v56 的能力暴露给 Rust core。
 *
 * ## 这一层只做 Rust 做不到的事
 *
 * 接收侧的随机写**不在这里**。数据块由 Rust 直接写进应用私有的暂存区
 * （`mobile-core/src/file_staging.rs`），一次都不跨语言边界。本文件保留的是三件
 * 平台独占的能力：
 *
 * 1. 读**平台独占的**发送源 —— Android 上「选目录发送」走
 *    `Directory.pickDirectoryAsync()` → SAF tree，子项是 `content://`，读权限在
 *    `ContentResolver` 手里，只有 expo-file-system 拿得到。
 *    **落在应用沙箱容器内的 `file://` 源不再经过这里**（2026-08-10 起）：Rust 侧按
 *    scheme + 归属直读，一次跨语言往返都不付。判据在
 *    `MobileFileAccessAdapter::owned_source_path`。
 * 2. 发布到 SAF 目标 —— 只有 `ContentResolver` 能在用户选的公共目录里建 document。
 * 3. 删除 SAF 上已落地的文件。
 *
 * `file://` 目标的发布（建目录 + rename）同样在 Rust 侧，不经过这里。
 *
 * ## 为什么接收不直接写 SAF
 *
 * 四条独立成立的理由，完整版在 `mobile-core/src/file_staging.rs` 的模块文档：
 * 部分 `DocumentsProvider` 返回**不可 seek** 的管道式 fd（`position()` 一律失败）、
 * SAF/FUSE 上随机写慢、不让用户目录出现半成品、暂存要与 checkpoint 同寿命跨重启存活。
 *
 * ⚠️ **归因更正（2026-08-10）**：这里此前写的是「SAF 的 fd 会被 provider 悄悄回收，
 * 下一次 `lseek` 得 `EBADF`」，佐证是 2026-08-07 那次「311 MB 稳定在 45 MB 处崩掉」。
 * 那个佐证是误诊——真因是 `expo-file-system` 的 `forContentURI` 不持有
 * `ParcelFileDescriptor`，GC 的 finalizer 关掉了 fd，而本仓修这条的 pnpm patch 从未
 * 进过 Android 构建（SDK 56 默认吃预编译 AAR）。**结论没变、理由换了。**
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
import { updatePublishProgress } from "@/core/foreground-service";
import { useTransferStore } from "@/stores/transfer-store";

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

/**
 * 发布进度的上报间隔。
 *
 * **沿用传输进度事件那条 200 ms 基线，不另发明节奏**——两者进的是同一批界面，
 * 节奏不一致只会让「传输条平滑、保存条一跳一跳」。
 * 每块都写 store 也不行：一块 4 MiB，一个几 GB 的文件会写出几千次 setState。
 */
const PUBLISH_REPORT_INTERVAL_MS = 200;

/**
 * 造一个按 [`PUBLISH_REPORT_INTERVAL_MS`] 节流的字节上报器。
 *
 * 上报只带 `(relativePath, written)`：本层拿到的 [`MobileFileMetadata`] 里没有
 * session_id / file_id，归属由先到的 `FilePublish{ phase: Started }` 事件在 store 里
 * 建好，这里只认领。
 *
 * **「这次发布值不值得播报」由 store 独家回答**，本层不复刻判据：`reportPublishBytes`
 * 返回「该 relativePath 此刻有没有条目」，有条目才更新通知。它一次覆盖两条曾在这里各存
 * 一份的规则 ——
 *
 * - **延迟揭示**：条目在 `PUBLISH_VISIBLE_AFTER_MS` 之后才出现，所以常数时间的发布
 *   （其余三端）不会把常驻通知刷成「正在保存 ⇄ 接收中」的频闪；
 * - **空文件**：Rust 的 `emit_publish_phase` 对 `size == 0` 根本不发 `started`，
 *   于是永远没有条目 —— 通知也就不会被一个含大量 `__init__.py` 的会话刷花。
 *
 * 此前这两条在本层各判一次，注释自认「两处判据必须一起改」；现在应用内与通知**同一个
 * 判据、同一处实现**，不可能再各说各的。
 */
function createPublishReporter(
  metadata: MobileFileMetadata,
): (written: number) => void {
  const total = Number(metadata.size);
  let lastReportAt = 0;
  return (written) => {
    const now = Date.now();
    // 终点那一次无条件发出去，否则最后一屏永远停在「差一块」的旧数字上。
    if (written < total && now - lastReportAt < PUBLISH_REPORT_INTERVAL_MS) {
      return;
    }
    lastReportAt = now;
    const visible = useTransferStore
      .getState()
      .reportPublishBytes(metadata.relativePath, written);
    // 通知是切走之后唯一能看到进度的面，所以它直连而不跟着 store 走一圈；但要不要出现
    // 这件事仍由 store 说了算。
    if (visible) {
      void updatePublishProgress(metadata.name, written, total);
    }
  };
}

function isSafUri(uri: string): boolean {
  return uri.startsWith("content://");
}

/**
 * expo-fs 读出来的 `Uint8Array` → 交给 Rust 的 `ArrayBuffer`（ubrn 只认后者）。
 *
 * 视图**已经独占整块 buffer** 时原样交出去，不再拷一遍：Android 侧交回来的正是刚 new
 * 出来的独占 ArrayBuffer 的完整视图（`JNIToJSIConverter.cpp` 每次都 `new ArrayBuffer(size)`），
 * 此时 `slice` 是纯白拷。
 *
 * **guard 不能删**：iOS 侧的 `readBytes` 实现未核实。若它哪天返回的是某个大 buffer 上的
 * 子视图，直接交出 `bytes.buffer` 就等于把整块内存（含别的 chunk 的字节）交给 Rust
 * ——那是静默的数据错乱，不是崩溃。
 */
function toOwnedArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  if (bytes.byteOffset === 0 && bytes.byteLength === bytes.buffer.byteLength) {
    return bytes.buffer as ArrayBuffer;
  }
  return bytes.buffer.slice(
    bytes.byteOffset,
    bytes.byteOffset + bytes.byteLength,
  ) as ArrayBuffer;
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

  /**
   * 读发送源的一段。
   *
   * **走到这里的只剩 Rust 读不了的源**：SAF `content://`、以及应用沙箱容器之外的
   * `file://`（iOS 的安全作用域 URL）。容器内的 `file://` 由 Rust 侧直读，
   * 分派判据在 `MobileFileAccessAdapter::owned_source_path`。
   */
  readSourceChunk(
    sourceId: string,
    offset: bigint,
    length: bigint,
  ): Promise<ArrayBuffer> {
    return wrapFfi(
      `read source chunk at offset ${offset} (${length} bytes)`,
      () => {
        // 读路径 source 是 expo-fs File / SAF content uri，统一 ReadOnly 模式打开。
        // handle 不缓存，于是**每块一次 open + seek + read + close**。
        //
        // ⚠️ 此前这里写的是「读取通常一次性 + 频繁 open/close 性能可以接受」，
        // **两句都被实测推翻**（2026-08-10）：一个文件按 256 KiB 分块要读几千次；
        // 而这条桥每块 9.25 ms（同机 Rust 直读 0.118 ms，约 78 倍），在那次 2 GB 发送里
        // 占发送侧 55% 的时间，且每块都硬阻塞单线程 tokio runtime，累计冻结网络事件
        // 循环约 39 s——是当次断链的嫌疑诱因。数据见仓库根的
        // `dev-notes/research/2026-08-10-mobile-bugs-diagnosis.md`。
        //
        // 同日起沙箱内的 `file://` 源已按 scheme 分派给 Rust 直读、整条桥都不走，
        // 但 **SAF 源仍全额付这笔开销**：`content://` 的读权限只在 `ContentResolver`
        // 手里，Rust 侧拿不到。所以「Android 选目录发送」慢是已知的、尚未消除的。
        // 唯一的下一步是按 sourceId 缓存 handle，但那要先有一个「这次传输读完了」的
        // 钩子据以 close——本层拿不到，故未做。
        const handle = new File(sourceId).open(FileMode.ReadOnly);
        try {
          handle.offset = Number(offset);
          // expo-fs readBytes 返回 Uint8Array；ubrn 期望 ArrayBuffer
          return toOwnedArrayBuffer(handle.readBytes(Number(length)));
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
      // **上报从这里起算，不是从拷贝循环起算**：下面的 `ensureSafTargetFile` 会逐层
      // `parent.list()` 全量枚举，用户选 Downloads 这类大目录时，拷贝**开始前**还有一段
      // 同样没有反馈的静止时间。先打一帧 0，界面与通知立刻进入「正在保存」。
      const report = createPublishReporter(metadata);
      report(0);
      const { file, dir } = ensureSafTargetFile(baseUri, metadata.relativePath);
      try {
        await copyIntoTarget(stagingUri, file, report);
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
 * **只用 `readBytes` / `writeBytes` 推进偏移，绝不 `setOffset`。** 这条规则不因
 * 2026-08-10 的归因更正而作废，它有独立理由：`DocumentsProvider.openDocument` 允许
 * 返回管道式的描述符（`ParcelFileDescriptor.createPipe`），那种 fd 上
 * `FileChannel.position()` **一律**失败，而 provider 是哪种由用户选的目录决定，
 * 我们无从预判。顺序写是唯一在所有 provider 上都成立的写法。
 *
 * ⚠️ **适用范围是「往外部位置写」，不是全文件。** 同文件的 `readSourceChunk` 每块都做
 * `handle.offset = …`，那**不是**违例：读源没有顺序这个选项——续传要从任意 offset 起，
 * 且 handle 不跨块缓存，每块都得先定位。写这边才有选择（publish 是一次整文件顺序拷贝），
 * 于是我们挑那个在所有 provider 上都成立的写法。
 * 推论一并记在这里：**源恰好来自管道式 provider 时，那条读路径会在定位上直接失败**——
 * 这是被接受的已知限制，本规则覆盖不到它。
 * （Rust 侧的同名读法 `read_at_sync` 一样是 `seek` + `read`，见 `mobile-core/src/file_access.rs`。）
 *
 * （此前这里把理由写成「`lseek` 正是 fd 被 provider 回收时炸掉的那个调用」——
 * 那次 `EBADF` 的真因是 expo 的 `forContentURI` 不持有 `ParcelFileDescriptor`，
 * 与 seek 无关；见本文件顶部的归因更正。）
 *
 * **不用 expo 的 `File.copy()`**，它的两条路径都不可用：
 * - copy 到具体文件（`isContainer=false`）会先 `deleteRecursively()` 再写，
 *   而 SAF document 删掉之后 uri 就失效了；
 * - copy 到目录（`isContainer=true`）会拿 **source 的文件名**建目标，
 *   而我们的暂存文件名是一串 hash。
 *
 * 每块之间让出一次事件循环：4 MiB 的同步 JSI 读写会阻塞 JS 线程几十毫秒，
 * 一个 300 MB 的文件连续搬完足以让界面卡死数秒。
 *
 * `report` 收每块之后的累计字节数（自带节流，见 [`createPublishReporter`]）——
 * 这个数本来就在循环里，此前只在抛错时被拼进错误串。
 */
async function copyIntoTarget(
  stagingUri: string,
  target: File,
  report: (written: number) => void,
): Promise<void> {
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
        report(written);
        // 这句 `setTimeout(0)` 是**故意的让出点**（见上面的说明），别删。
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
