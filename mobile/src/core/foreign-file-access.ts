/**
 * ForeignFileAccess 实现：把 expo-file-system v55 的 next API 暴露给 Rust core。
 *
 * 用 `File.open()` 拿 FileHandle，靠 `offset` + `readBytes/writeBytes` 真正
 * 按 chunk 读写，不会把整个文件加载到内存。
 */

import { Directory, File } from "expo-file-system";
import type {
  ForeignFileAccess,
  MobileFileMetadata,
  MobileSaveLocation,
} from "react-native-swarmdrop-core";

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
      saveDir: undefined,
    };
  }

  async readSourceChunk(
    sourceId: string,
    offset: bigint,
    length: bigint,
  ): Promise<ArrayBuffer> {
    const handle = new File(sourceId).open();
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
  }

  async createSink(metadata: MobileFileMetadata): Promise<string> {
    const file = ensureSinkFile(metadata, /* truncate */ true);
    const sinkId = file.uri;
    this.sinks.set(sinkId, { metadata, file });
    return sinkId;
  }

  async openOrCreateSink(metadata: MobileFileMetadata): Promise<string> {
    // 如果文件已存在则保留（断点续传），否则创建空文件
    const file = ensureSinkFile(metadata, /* truncate */ false);
    const sinkId = file.uri;
    this.sinks.set(sinkId, { metadata, file });
    return sinkId;
  }

  async writeSinkChunk(
    sinkId: string,
    offset: bigint,
    data: ArrayBuffer,
  ): Promise<void> {
    const sink = this.sinks.get(sinkId);
    if (!sink) {
      throw new Error(`sink 不存在: ${sinkId}`);
    }
    const handle = sink.file.open();
    try {
      handle.offset = Number(offset);
      handle.writeBytes(new Uint8Array(data));
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
 * 用 metadata.saveDir（core 注入的用户选择目录）+ relativePath 拼最终文件 File。
 *
 * core/host.rs 现在保证 receive 路径调 create_sink 时一定会塞 save_dir，
 * 不再有"全局共享 sink + None 路径"的隐患。
 */
function ensureSinkFile(metadata: MobileFileMetadata, truncate: boolean): File {
  const baseUri = saveLocationUri(metadata.saveDir);
  const baseDir = new Directory(baseUri);
  if (!baseDir.exists) {
    baseDir.create({ intermediates: true });
  }
  const file = new File(baseDir, metadata.relativePath);
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

function saveLocationUri(saveDir: MobileSaveLocation | undefined): string {
  if (!saveDir) {
    throw new Error(
      "MobileFileMetadata.saveDir 缺失：core 未注入用户选择的保存目录",
    );
  }
  return saveDir.inner.path;
}
