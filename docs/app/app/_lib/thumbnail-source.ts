// Web 侧的缩略图取图源：`previewSource` → 字节。
//
// 缩放 / 缓存 / 并发 / 视口触发全在 `@swarmdrop/file-browser` 的 `use-thumbnail.ts`；
// 这里只负责「把 `previewSource` 换成字节」这一件事——那正是三端唯一不同的一步
// （桌面根本不走管线，它的 `previewUrl` 直接能渲染；移动走 expo-image 原生解码）。

import { getNode } from "./node-runtime";
import { detectSecureContext } from "./secure-context";

/**
 * OPFS 可用性。**惰性算一次就够**：`isSecureContext` 与 `navigator.storage` 在一个文档的
 * 生命周期内不会变，而取图源是每张图都要调的。
 */
let opfsUsable: boolean | null = null;
function isOpfsUsable(): boolean {
  if (opfsUsable === null) {
    const context = detectSecureContext();
    opfsUsable = context.isSecure && context.hasStorage;
  }
  return opfsUsable;
}

/**
 * 收件箱的取图源：OPFS 相对路径 → `File`。
 *
 * **必须是模块级函数**（稳定引用）——它进 `useThumbnail` 的 effect 依赖，每次渲染换一个
 * 新的会让每一帧都重跑一遍取图。
 *
 * 三种拿不到就直接给 `null`，由卡片降级到类型图标，一律不抛：
 *
 * 1. **非 secure origin**（http 私网 IP）→ OPFS 整个不可用。这里提前判掉而不是让
 *    `open_file` 报错，是因为那条路径每张图都要付一次 5s 超时的等待。
 * 2. **节点没起来** → `getNode()` 为 null。收件箱是能在节点停止时浏览的（数据在 IndexedDB
 *    里），但取字节要经内核。
 * 3. **文件不在了** → `open_file` reject（会话未完成 / 已被删）。
 */
export async function opfsThumbnailSource(previewSource: string): Promise<Blob | null> {
  if (!isOpfsUsable()) return null;
  const node = getNode();
  if (!node) return null;
  try {
    return await node.open_file(previewSource);
  } catch {
    return null;
  }
}

/**
 * 发送侧的取图源：用户刚选的那个浏览器 `File`，**不在 OPFS 里**。
 *
 * 待发文件的字节只存在于 `PendingFile.file` 这个内存句柄上，没有任何路径能指向它。所以
 * `previewSource` 存的是自增序号，取图靠它回查当前那份 `PendingFile[]`。
 *
 * **`files` 走 ref 而不是闭包捕获**：`createPendingFileThumbnailSource(files)` 那种形态
 * 每次 `files` 变（加一个文件、删一个文件）都会产出新引用，而它是 `useThumbnail` 的 effect
 * 依赖——于是每加一个文件，**已渲染的每张卡片都要重跑一遍 effect**。现在返回的 resolver
 * 引用恒定，只有 ref 里的内容在变。
 */
export function createPendingFileThumbnailSource(): {
  resolver: (previewSource: string) => Promise<Blob | null>;
  setFiles: (files: readonly { id: number; file: File }[]) => void;
} {
  let current: readonly { id: number; file: File }[] = [];
  return {
    resolver: async (previewSource) =>
      current.find(({ id }) => String(id) === previewSource)?.file ?? null,
    setFiles: (files) => {
      current = files;
    },
  };
}
