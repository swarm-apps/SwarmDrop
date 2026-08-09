import { describe, expect, it } from "vitest";
import {
  allDownloadKey,
  directoryDownloadKey,
  fileDownloadKey,
  parseDownloadKey,
} from "./view-types";

const ITEM = "b7f3c0e2-0000-4000-8000-000000000001";
const OTHER = "b7f3c0e2-0000-4000-8000-000000000002";

/**
 * 下载键是**编解码对**：三种目标压成一个字符串（`useKeyedAsyncAction` 只认键），再由
 * `parseDownloadKey` 还原成「这是谁」。pending 的 spinner 与失败卡片的标题两条路径都
 * 依赖这次往返，而它们的失效方式都是静默的（转圈不停 / 错误不显示）。
 */
describe("download key round-trip", () => {
  it("restores every target kind", () => {
    expect(parseDownloadKey(ITEM, allDownloadKey(ITEM))).toEqual({ kind: "all" });
    expect(parseDownloadKey(ITEM, fileDownloadKey(ITEM, 7))).toEqual({
      kind: "file",
      fileId: 7,
    });
    expect(
      parseDownloadKey(ITEM, directoryDownloadKey(ITEM, "photos/2024/")),
    ).toEqual({ kind: "directory", relativePath: "photos/2024/" });
  });

  /** 一个 hook 实例服务收件箱里的所有条目，键集合是全局的——读的人必须能筛掉别人的。 */
  it("rejects keys that belong to another item", () => {
    expect(parseDownloadKey(OTHER, fileDownloadKey(ITEM, 7))).toBeNull();
    expect(parseDownloadKey(OTHER, allDownloadKey(ITEM))).toBeNull();
    expect(
      parseDownloadKey(OTHER, directoryDownloadKey(ITEM, "photos/")),
    ).toBeNull();
  });

  /**
   * 只有**纯数字**后缀才是文件键。`Number("")` 是 0，所以裸前缀曾被认成「0 号文件」——
   * 那会往 `pendingIds` 里塞一个不存在的展示 id，spinner 挂在一个谁也不是的行上。
   */
  it("does not mistake a malformed suffix for file 0", () => {
    expect(parseDownloadKey(ITEM, `${ITEM}:`)).toBeNull();
    expect(parseDownloadKey(ITEM, `${ITEM}: 7 `)).toBeNull();
    expect(parseDownloadKey(ITEM, `${ITEM}:1e3`)).toBeNull();
    expect(parseDownloadKey(ITEM, `${ITEM}:0`)).toEqual({ kind: "file", fileId: 0 });
  });

  /** 目录名里带冒号、或长得像 `all` —— 都不该被认成另一种目标。 */
  it("does not confuse a directory whose name looks like another key", () => {
    expect(parseDownloadKey(ITEM, directoryDownloadKey(ITEM, "all/"))).toEqual({
      kind: "directory",
      relativePath: "all/",
    });
    expect(parseDownloadKey(ITEM, directoryDownloadKey(ITEM, "a:b/"))).toEqual({
      kind: "directory",
      relativePath: "a:b/",
    });
  });
});
