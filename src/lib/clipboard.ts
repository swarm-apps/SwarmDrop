/**
 * clipboard —— 桌面端剪贴板封装
 *
 * 读写统一走 Tauri clipboard-manager 插件（原生系统剪贴板），不用 WebView 的
 * Clipboard API：
 * - 写：WebView 的 `writeText` 会触发浏览器权限申请弹窗，在原生 app 里很怪异。
 * - 读：WebView 的 `readText` 在 WKWebView / WebKitGTK 上通常要求 transient
 *   activation（`focus` 事件不算用户手势），跨平台行为不一致；且失败只能靠 catch
 *   吞掉，感知能力实际不可用时没有任何信号。
 *
 * 两个方向都收口在本文件，调用方不直接碰 plugin 也不碰 `navigator`——
 * `pnpm check:clipboard` 会拦住绕过本封装的写法。
 */

import {
  readText as readSystemClipboard,
  writeText,
} from "@tauri-apps/plugin-clipboard-manager";

/** 宿主剪贴板能力只停留在 UI 边界，不得穿过 IPC、core 或 wire。 */
export interface ClipboardPort {
  readText(): Promise<string>;
  writeText(text: string): Promise<void>;
}

export const clipboard: ClipboardPort = {
  readText: readSystemClipboard,
  writeText,
};

/** 复制文本到系统剪贴板。 */
export async function copyText(text: string): Promise<void> {
  await clipboard.writeText(text);
}

/**
 * 读取系统剪贴板文本。
 *
 * **剪贴板为空或内容非文本时会 reject**（插件底层是 arboard 的
 * `ContentNotAvailable`），无法与「权限缺失」等真故障区分——调用方一律按
 * 「读不到」处理即可，不要据此推断原因。
 */
export async function readText(): Promise<string> {
  return clipboard.readText();
}
