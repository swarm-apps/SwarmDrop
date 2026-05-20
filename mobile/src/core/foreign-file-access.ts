/**
 * ForeignFileAccess 实现：把 expo-file-system v56 的 next API 暴露给 Rust core。
 *
 * 用 `File.open(mode)` 拿 FileHandle，靠 `offset` + `readBytes/writeBytes` 真正
 * 按 chunk 读写，不会把整个文件加载到内存。
 *
 * ## SAF (content://) 支持
 *
 * Android 用户在「设置 → 接收位置」选系统目录时，saveDir 是 SAF
 * `content://com.android.externalstorage.documents/tree/...`。expo-file-system 56
 * 通过 ContentResolver.openFileDescriptor + FileChannel 真正支持 SAF chunk write，
 * 但有两个限制（来自 expo 文档）：
 *
 * 1. SAF 不支持 `FileMode.ReadWrite`，只能 `WriteOnly`
 * 2. SAF 不能拼路径 `new File(dir, "a/b/c.txt")`，要 `dir.createDirectory(name)`
 *    递归建子目录，叶子用 `dir.createFile(name, null)`
 *
 * 更关键：SAF 的 "w" mode 在大多数 DocumentsProvider 下会 truncate-on-open。
 * 因此 sink 生命周期内必须**保持 handle 打开**，所有 chunk 复用同一个 handle，
 * 避免每次 open 都丢失之前写入的内容。
 *
 * file:// 路径走 `ReadWrite` 模式 + 持久 handle，性能上比每 chunk open/close 也更优。
 */

import { Directory, File, type FileHandle, FileMode } from "expo-file-system";
import {
  FfiError,
  type ForeignFileAccess,
  type MobileFileMetadata,
  type MobileSaveLocation,
} from "react-native-swarmdrop-core";

/**
 * 任意 JS error → `FfiError.Io` —— 必须包成 uniffi enum 形状，否则 uniffi 在
 * lift callback return 时认不出错误类型，会走 `handle_callback_unexpected_error`
 * 触发 Rust panic（catch_unwind 后 abort，日志只有 "Rust panic" 没有源信息）。
 */
async function wrapFfi<T>(fn: () => Promise<T> | T): Promise<T> {
  try {
    return await fn();
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw FfiError.Io.new(message);
  }
}

interface OpenSink {
  metadata: MobileFileMetadata;
  file: File;
  /** sink 生命周期内保持打开的 handle；SAF 下不能每 chunk 重新 open（会 truncate） */
  handle: FileHandle;
}

function isSafUri(uri: string): boolean {
  return uri.startsWith("content://");
}

export class ExpoFileAccess implements ForeignFileAccess {
  /** 已 create 但未 finalize 的 sink；持有 FileHandle 直到 finalize/cleanup */
  private readonly sinks = new Map<string, OpenSink>();

  sourceMetadata(sourceId: string): Promise<MobileFileMetadata> {
    return wrapFfi(() => {
      const file = new File(sourceId);
      if (!file.exists) {
        throw new Error(`source 不存在: ${sourceId}`);
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
    return wrapFfi(() => {
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
    });
  }

  createSink(metadata: MobileFileMetadata): Promise<string> {
    return wrapFfi(() => this.openSink(metadata, /* truncate */ true));
  }

  openOrCreateSink(metadata: MobileFileMetadata): Promise<string> {
    return wrapFfi(() => this.openSink(metadata, /* truncate */ false));
  }

  writeSinkChunk(
    sinkId: string,
    offset: bigint,
    data: ArrayBuffer,
  ): Promise<void> {
    return wrapFfi(() => {
      const sink = this.sinks.get(sinkId);
      if (!sink) {
        throw new Error(`sink 不存在: ${sinkId}`);
      }
      sink.handle.offset = Number(offset);
      sink.handle.writeBytes(new Uint8Array(data));
    });
  }

  async finalizeSink(sinkId: string): Promise<void> {
    // host 已按 chunk 写入完整文件；core 端通过 BLAKE3 校验，
    // 若失败会调 cleanup_sink。这里关掉 handle + 清掉内存引用。
    const sink = this.sinks.get(sinkId);
    if (sink) {
      this.sinks.delete(sinkId);
      try {
        sink.handle.close();
      } catch {
        // best-effort：handle 可能已被系统回收，忽略
      }
    }
  }

  cleanupSink(sinkId: string): Promise<void> {
    return wrapFfi(() => {
      const sink = this.sinks.get(sinkId);
      if (!sink) return;
      this.sinks.delete(sinkId);
      try {
        sink.handle.close();
      } catch {
        // best-effort
      }
      if (sink.file.exists) {
        sink.file.delete();
      }
    });
  }

  private openSink(metadata: MobileFileMetadata, truncate: boolean): string {
    const baseUri = saveLocationUri(metadata.saveDir);
    const saf = isSafUri(baseUri);
    const file = saf
      ? ensureSafSinkFile(baseUri, metadata.relativePath, truncate)
      : ensureLocalSinkFile(baseUri, metadata.relativePath, truncate);

    // SAF 不支持 ReadWrite；走 WriteOnly。WriteOnly 模式 cursor 在头部，能 seek
    // （FileChannel-from-FileOutputStream 支持 position），所以 chunk write OK。
    // 必须保持 handle 打开整个 sink 生命周期：SAF "w" 模式 open 时会 truncate，
    // 每 chunk 重新 open 会丢失之前内容。
    const mode = saf ? FileMode.WriteOnly : FileMode.ReadWrite;
    const handle = file.open(mode);
    const sinkId = file.uri;
    this.sinks.set(sinkId, { metadata, file, handle });
    return sinkId;
  }
}

/**
 * file:// 路径：用 metadata.saveDir + relativePath 拼最终 File，递归建父目录。
 */
function ensureLocalSinkFile(
  baseUri: string,
  relativePath: string,
  truncate: boolean,
): File {
  const baseDir = new Directory(baseUri);
  if (!baseDir.exists) {
    baseDir.create({ intermediates: true });
  }
  const file = new File(baseDir, relativePath);
  const parent = file.parentDirectory;
  if (!parent.exists) {
    parent.create({ intermediates: true });
  }
  if (truncate || !file.exists) {
    if (file.exists) {
      file.delete();
    }
    file.create();
  }
  return file;
}

/**
 * SAF tree URI 路径：不能拼路径，要逐层 createDirectory + 叶子 createFile。
 *
 * relativePath 形如 "SwarmNote/sub/foo.txt"。在 SAF tree 下顺次寻找/创建
 * 「SwarmNote」「sub」目录，最后在 sub 下 createFile("foo.txt", null)。
 */
function ensureSafSinkFile(
  baseUri: string,
  relativePath: string,
  truncate: boolean,
): File {
  const segments = relativePath.split("/").filter(Boolean);
  if (segments.length === 0) {
    throw new Error(`SAF sink relativePath 为空: ${relativePath}`);
  }
  const fileName = segments[segments.length - 1];
  const dirSegments = segments.slice(0, -1);

  let currentDir = new Directory(baseUri);
  for (const seg of dirSegments) {
    const existing = findChildDirectory(currentDir, seg);
    currentDir = existing ?? currentDir.createDirectory(seg);
  }

  const existingFile = findChildFile(currentDir, fileName);
  if (existingFile && truncate) {
    existingFile.delete();
  } else if (existingFile) {
    return existingFile;
  }
  // mimeType 传 null 让 DocumentsProvider 按文件名后缀推断
  return currentDir.createFile(fileName, null);
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
      "MobileFileMetadata.saveDir 缺失：core 未注入用户选择的保存目录",
    );
  }
  return saveDir.inner.path;
}
