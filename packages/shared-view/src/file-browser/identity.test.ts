import { describe, expect, it } from "vitest";
import {
  getParentPath,
  isPathInsideDirectory,
  normalizeDirectoryPath,
  normalizeRelativePath,
} from "./identity";

describe("normalizeRelativePath", () => {
  it("反斜杠转正斜杠，去掉空段", () => {
    expect(normalizeRelativePath("a\\b//c.txt")).toBe("a/b/c.txt");
  });

  // `..` 会被各端当作落盘路径使用，留着就是路径穿越。
  it("过滤掉 . 与 ..", () => {
    expect(normalizeRelativePath("a/./b/../c.txt")).toBe("a/b/c.txt");
    expect(normalizeRelativePath("../../etc/passwd")).toBe("etc/passwd");
  });

  it("路径为空时回落到文件名，再空则给 file", () => {
    expect(normalizeRelativePath("", "photo.jpg")).toBe("photo.jpg");
    expect(normalizeRelativePath("///", "photo.jpg")).toBe("photo.jpg");
    expect(normalizeRelativePath("")).toBe("file");
    expect(normalizeRelativePath("", "")).toBe("file");
  });
});

describe("normalizeDirectoryPath", () => {
  it("与文件版同规则，但空就是空（不回落）", () => {
    expect(normalizeDirectoryPath("a\\b")).toBe("a/b");
    expect(normalizeDirectoryPath("")).toBe("");
  });
});

describe("getParentPath", () => {
  it("顶层文件的父目录是空串", () => {
    expect(getParentPath("a.txt")).toBe("");
  });

  it("取到最后一个分隔符之前", () => {
    expect(getParentPath("a/b/c.txt")).toBe("a/b");
  });
});

describe("isPathInsideDirectory", () => {
  it("目录自身与其后代都算在内", () => {
    expect(isPathInsideDirectory("a/b", "a/b")).toBe(true);
    expect(isPathInsideDirectory("a/b/c.txt", "a/b")).toBe(true);
  });

  it("同前缀但不同目录不算（a/bc 不在 a/b 下）", () => {
    expect(isPathInsideDirectory("a/bc.txt", "a/b")).toBe(false);
  });

  // 空目录前缀会匹配上所有文件，而调用点通常是「移除这个目录」这类破坏性操作。
  it("目录为空时恒为 false", () => {
    expect(isPathInsideDirectory("a/b/c.txt", "")).toBe(false);
  });
});
