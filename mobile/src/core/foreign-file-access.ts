/**
 * ForeignFileAccess 实现：把 expo-file-system v55 的 next API 暴露给 Rust core。
 *
 * 用 `File.open()` 拿 FileHandle，靠 `offset` + `readBytes/writeBytes` 真正
 * 按 chunk 读写，不会把整个文件加载到内存。
 */

import { File, Directory, Paths } from "expo-file-system";
import type {
  ForeignFileAccess,
  MobileFileMetadata,
} from "react-native-swarmdrop-core";

const SAVE_SUBDIR = "SwarmDrop";

interface OpenSink {
  metadata: MobileFileMetadata;
  file: File;
}

export class ExpoFileAccess implements ForeignFileAccess {
  /** 已 create 但未 finalize 的 sink；不持有 FileHandle，写入时按需 open */
  private readonly sinks = new Map<string, OpenSink>();

  async sourceMetadata(sourceId: string): Promise<MobileFileMetadata> {
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
    };
  }

  async readSourceChunk(
    sourceId: string,
    offset: bigint,
    length: bigint,
  ): Promise<Uint8Array> {
    const handle = new File(sourceId).open();
    try {
      handle.offset = Number(offset);
      return handle.readBytes(Number(length));
    } finally {
      handle.close();
    }
  }

  async createSink(metadata: MobileFileMetadata): Promise<string> {
    const file = ensureSinkFile(metadata.relativePath, /* truncate */ true);
    const sinkId = file.uri;
    this.sinks.set(sinkId, { metadata, file });
    return sinkId;
  }

  async openOrCreateSink(metadata: MobileFileMetadata): Promise<string> {
    // 如果文件已存在则保留（断点续传），否则创建空文件
    const file = ensureSinkFile(metadata.relativePath, /* truncate */ false);
    const sinkId = file.uri;
    this.sinks.set(sinkId, { metadata, file });
    return sinkId;
  }

  async writeSinkChunk(
    sinkId: string,
    offset: bigint,
    data: Uint8Array,
  ): Promise<void> {
    const sink = this.sinks.get(sinkId);
    if (!sink) {
      throw new Error(`sink 不存在: ${sinkId}`);
    }
    const handle = sink.file.open();
    try {
      handle.offset = Number(offset);
      handle.writeBytes(data);
    } finally {
      handle.close();
    }
  }

  async finalizeSink(sinkId: string): Promise<void> {
    // host 已按 chunk 写入完整文件；core 端通过 BLAKE3 校验，
    // 若失败会调 cleanup_sink。这里只清掉内存引用。
    this.sinks.delete(sinkId);
  }

  async cleanupSink(sinkId: string): Promise<void> {
    const sink = this.sinks.get(sinkId);
    this.sinks.delete(sinkId);
    if (sink && sink.file.exists) {
      sink.file.delete();
    }
  }
}

/**
 * 把 metadata.relativePath 解析成 documentDirectory/SwarmDrop/<path> 的 File。
 * 必要时创建父目录与空文件。
 */
function ensureSinkFile(relativePath: string, truncate: boolean): File {
  const baseDir = new Directory(Paths.document, SAVE_SUBDIR);
  if (!baseDir.exists) {
    baseDir.create({ intermediates: true });
  }
  const file = new File(baseDir, relativePath);
  // 确保父目录存在（relativePath 可能含子目录）
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
