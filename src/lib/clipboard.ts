/**
 * clipboard —— 桌面端剪贴板封装
 *
 * 统一走 Tauri clipboard-manager 插件（原生系统剪贴板），避免 WebView 里
 * `navigator.clipboard.writeText` 触发浏览器权限申请弹窗（桌面 app 里体验很怪异）。
 */

import { writeText } from "@tauri-apps/plugin-clipboard-manager";

/** 复制文本到系统剪贴板。 */
export async function copyText(text: string): Promise<void> {
  await writeText(text);
}
