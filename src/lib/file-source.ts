/**
 * file-source
 * 文件系统路径 → `FileSource` 的唯一构造点。
 *
 * **刻意独立成模块，不放 `file-picker.ts`**：那个模块顶层 import 了
 * `@tauri-apps/plugin-dialog` 与 `@tauri-apps/api/path`，而这里的消费者有
 * app-shell 代码（`external-open-handler`）和纯逻辑模块（`transfer-actions`），
 * 它们只是要造个 DTO，不该为此把选择器与路径插件拖进自己的 chunk。
 */

import type { FileSource } from "@/lib/bindings";

function pathToSource(path: string): FileSource {
  return { type: "path", path };
}

/**
 * 五条入口共用：选文件、选文件夹、拖放、外部打开（share-target）、重新发送。
 * `FileSource` 若将来长出必填字段，这里是唯一要改的地方。
 */
export function pathsToSources(paths: string[]): FileSource[] {
  return paths.map(pathToSource);
}
