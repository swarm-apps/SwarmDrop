/**
 * 路径归一与展示 ID 构造 —— 三端共用。
 *
 * 从 `mobile/src/core/file-browser-identity.ts` 上移（它在移动端已经被抽出组件目录，
 * 只是抽到了移动端自己的 `core/`），并入桌面 `tree-data.ts` 里的 `getParentPath`。
 *
 * ID 带 scope 前缀是有意的：同一个 `fileId` 会在「入站 offer」与「传输会话」两处出现，
 * 不加前缀两者的展示 ID 会撞车，React 的 key 与树的节点表都会错乱。
 */

/**
 * 相对路径归一：反斜杠转正斜杠、去掉空段与 `.` / `..`。
 *
 * **双参版为准**（取自移动端）：路径为空时回落到文件名，再空则给 `"file"`。
 * 桌面此前是单参版，遇到空 `relativePath` 会产出空串，建树时那个文件会挂在根上且没有名字。
 *
 * `..` 必须过滤掉，不能保留——它是路径穿越的载体，而这个字符串会被各端当作落盘的
 * 相对路径使用。
 */
export function normalizeRelativePath(path: string, fallbackName = "file"): string {
  return normalizeSegments(path) || fallbackName || "file";
}

/** 目录路径归一。与文件版的差别只在「空就是空」，不回落文件名。 */
export function normalizeDirectoryPath(path: string): string {
  return normalizeSegments(path);
}

/**
 * 两个归一函数的共同主体。
 *
 * **用 `replace(/\\/g, …)` 而不是 `replaceAll`**：本包发布的是 TS 源，由三端各自的
 * tsconfig 编译，而它们的 `lib` 不一定到 ES2021（桌面就没到）。包自己的 tsconfig 是
 * ES2022，所以 `tsc -p packages/shared-view` 会放行——那道门只覆盖包自身，
 * 三端 typecheck 用的是各自的 lib（见 README 的门禁表）。
 */
function normalizeSegments(path: string): string {
  return (path ?? "")
    .replace(/\\/g, "/")
    .split("/")
    .filter((segment) => segment.length > 0 && segment !== "." && segment !== "..")
    .join("/");
}

/** 取父目录路径（不带尾斜杠）；顶层文件返回空串。 */
export function getParentPath(relativePath: string): string {
  const normalized = normalizeDirectoryPath(relativePath);
  const lastSeparator = normalized.lastIndexOf("/");
  return lastSeparator < 0 ? "" : normalized.slice(0, lastSeparator);
}

/* ─── 展示 ID ─── */

/** 发送侧已选文件：键是来源路径 / URI。 */
export function selectedFileId(sourceId: string): string {
  return `source:${sourceId}`;
}

/** 传输会话内的文件（入站 offer 与传输投影共用——两者是同一批文件的两个阶段）。 */
export function sessionFileId(sessionId: string, fileId: number): string {
  return `session:${sessionId}:file:${fileId}`;
}

/** 收件箱条目内的文件。 */
export function inboxFileId(itemId: string, fileId: number): string {
  return `inbox:${itemId}:file:${fileId}`;
}

/**
 * 某个相对路径是否落在某个目录之下（含目录自身）。
 *
 * 目录为空时**恒为 false**：空目录前缀会匹配上所有文件，而调用点通常是「移除这个目录」
 * 这类破坏性操作。
 */
export function isPathInsideDirectory(
  relativePath: string,
  relativeDirectory: string,
): boolean {
  const path = normalizeRelativePath(relativePath);
  const directory = normalizeDirectoryPath(relativeDirectory);
  return (
    directory.length > 0 && (path === directory || path.startsWith(`${directory}/`))
  );
}
