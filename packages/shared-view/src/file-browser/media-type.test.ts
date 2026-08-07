import { describe, expect, it } from "vitest";
import { fileCategory, isImageFile, isVideoFile } from "./media-type";

describe("isImageFile / isVideoFile", () => {
  it("大小写不敏感，按最后一段扩展名判定", () => {
    expect(isImageFile("PHOTO.JPG")).toBe(true);
    expect(isImageFile("a.tar.gz")).toBe(false);
    expect(isVideoFile("clip.MOV")).toBe(true);
  });

  it("没有扩展名的文件不属于任何媒体类型", () => {
    expect(isImageFile("README")).toBe(false);
    expect(isVideoFile("Makefile")).toBe(false);
  });

  // 桌面 file-icon.ts 那份旧表缺这几个，导致同一文件「显示什么图标」与
  // 「要不要生成缩略图」两处答案矛盾。合并后必须都在。
  it("覆盖两端旧表各自缺的那些扩展名", () => {
    for (const name of ["a.heic", "a.heif", "a.avif", "a.tiff", "a.ico"]) {
      expect(isImageFile(name)).toBe(true);
    }
    for (const name of ["a.m4v", "a.wmv", "a.3gp"]) {
      expect(isVideoFile(name)).toBe(true);
    }
  });
});

describe("fileCategory", () => {
  it("按大类归档", () => {
    expect(fileCategory("a.png")).toBe("image");
    expect(fileCategory("a.mp4")).toBe("video");
    expect(fileCategory("a.mp3")).toBe("audio");
    expect(fileCategory("a.zip")).toBe("archive");
    expect(fileCategory("a.rs")).toBe("code");
    expect(fileCategory("a.pdf")).toBe("document");
  });

  it("未知扩展名与无扩展名都归 other", () => {
    expect(fileCategory("a.qwerty")).toBe("other");
    expect(fileCategory("LICENSE")).toBe("other");
  });

  it("与 isImageFile / isVideoFile 保持同源，不会互相矛盾", () => {
    for (const name of ["a.png", "a.svg", "a.ico", "a.heic"]) {
      expect(isImageFile(name)).toBe(true);
      expect(fileCategory(name)).toBe("image");
    }
    for (const name of ["a.mkv", "a.3gp"]) {
      expect(isVideoFile(name)).toBe(true);
      expect(fileCategory(name)).toBe("video");
    }
  });
});
