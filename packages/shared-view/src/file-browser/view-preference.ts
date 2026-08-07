/**
 * 文件浏览器视图偏好的**默认值与归一** —— 三端共用。
 *
 * `FileBrowserScope` / `FileBrowserView` 两个类型已经在 L1 了，但默认值与「磁盘上的值坏了
 * 怎么办」此前三端各写一份（桌面内联字面量、移动 `mergeFileBrowserViews`、Web 又抄了第三份）。
 * scope 键名写错类型系统会拦，**默认值漂了没有任何东西会拦**——而「收件箱默认网格、发送与
 * 传输默认树形」是一条产品决策，不是各端的自由。
 *
 * 各端 store 只负责「存哪里」（tauri-plugin-store / AsyncStorage / localStorage）。
 * 体例同本包已有的 `emptyDeviceOrganization` + `normalizeDeviceOrganization`。
 */

import type { FileBrowserScope, FileBrowserView } from "./types";

/**
 * 各场景的默认视图。
 *
 * 收件箱默认**网格**——那是「看收到了什么」，缩略图比路径有用；发送与传输默认**树形**——
 * 那两处用户关心的是「我选的目录结构对不对」「哪个文件在传」。
 */
export const DEFAULT_FILE_BROWSER_VIEWS: Readonly<
  Record<FileBrowserScope, FileBrowserView>
> = {
  send: "tree",
  inbox: "grid",
  transfer: "tree",
};

/**
 * 归一持久化下来的视图偏好：缺字段、多字段、被手改坏的值一律逐 scope 退回默认。
 *
 * 返回的**永远是一份新对象**，调用方可以直接放进 store 而不必担心与磁盘上那份共享引用。
 */
export function normalizeFileBrowserViews(
  persisted: unknown,
): Record<FileBrowserScope, FileBrowserView> {
  const source = (persisted ?? {}) as Partial<Record<FileBrowserScope, unknown>>;
  const result = { ...DEFAULT_FILE_BROWSER_VIEWS };
  for (const scope of Object.keys(result) as FileBrowserScope[]) {
    const value = source[scope];
    if (value === "tree" || value === "grid") result[scope] = value;
  }
  return result;
}
