import { describe, expect, it } from "vitest";
import {
  fromEnumeratedFiles,
  fromInboxFiles,
  fromOfferFiles,
  fromTransferProjectionFiles,
} from "./adapters";

describe("file browser adapters", () => {
  it("maps scanned files without creating a preview URL", () => {
    const [mapped] = fromEnumeratedFiles([{
      name: "note.txt",
      relativePath: "docs\\note.txt",
      source: { type: "path", path: "C:\\work\\docs\\note.txt" },
      size: 12,
    }]);
    expect(mapped).toMatchObject({
      id: "send:C:/work/docs/note.txt",
      relativePath: "docs/note.txt",
      localPath: "C:\\work\\docs\\note.txt",
      status: "idle",
    });
    expect(mapped.previewUrl).toBeUndefined();
  });

  it("maps offer files to stable read-only items and ignores directory markers", () => {
    const mapped = fromOfferFiles([
      { fileId: 7, name: "a.txt", relativePath: "dir/a.txt", size: 4, isDirectory: false },
      { fileId: 8, name: "dir", relativePath: "dir", size: 0, isDirectory: true },
    ]);
    expect(mapped).toHaveLength(1);
    expect(mapped[0]).toMatchObject({ id: "offer:7", fileId: 7, status: "idle" });
  });

  it("maps transfer status, progress and explicit error precedence", () => {
    const files = [{ fileId: 2, name: "a.bin", relativePath: "a.bin", size: 100, transferredBytes: 10 }];
    const progress = {
      sessionId: "s",
      direction: "send" as const,
      totalFiles: 1,
      completedFiles: 0,
      totalBytes: 100,
      transferredBytes: 42,
      speed: null,
      eta: null,
      files: [{ fileId: 2, name: "a.bin", size: 100, transferred: 42, status: "transferring" as const }],
    };
    expect(fromTransferProjectionFiles(files, { progress })[0]).toMatchObject({
      id: "transfer:2",
      status: "transferring",
      progress: 42,
    });
    expect(fromTransferProjectionFiles(files, { progress, errorFileIds: new Set([2]) })[0].status).toBe("error");
  });

  it("maps inbox missing state and only accepts caller-provided preview URLs", () => {
    const files = [
      { id: 1, transferFileId: 4, relativePath: "photo.png", name: "photo.png", size: 9, checksum: "a", localPath: "safe/photo.png", missing: false },
      { id: 2, transferFileId: null, relativePath: "gone.png", name: "gone.png", size: 3, checksum: "b", localPath: "safe/gone.png", missing: true },
    ];
    const mapped = fromInboxFiles(files, { getPreviewUrl: (file) => `asset://${file.localPath}` });
    expect(mapped[0]).toMatchObject({
      id: "inbox:1",
      localPath: "safe/photo.png",
      previewUrl: "asset://safe/photo.png",
      status: "completed",
    });
    expect(mapped[1]).toMatchObject({ id: "inbox:2", status: "missing" });
    expect(mapped[1].previewUrl).toBeUndefined();
  });
});
