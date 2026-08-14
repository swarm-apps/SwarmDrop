import * as Clipboard from "expo-clipboard";

/**
 * 移动端剪贴板宿主适配。调用点只能在显式用户手势内使用它，避免把系统能力泄漏进共享 core。
 */
export interface ClipboardPort {
  readText(): Promise<string>;
  writeText(text: string): Promise<void>;
}

export const clipboard: ClipboardPort = {
  readText: Clipboard.getStringAsync,
  writeText: async (text) => {
    await Clipboard.setStringAsync(text);
  },
};
