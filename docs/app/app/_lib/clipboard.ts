"use client";

/** Web 侧只在明确用户手势内调用；权限拒绝由调用方保留编辑器内容并给出非阻塞反馈。 */
export interface ClipboardPort {
  readText(): Promise<string>;
  writeText(text: string): Promise<void>;
}

export const clipboard: ClipboardPort = {
  readText: () => navigator.clipboard.readText(),
  writeText: (text) => navigator.clipboard.writeText(text),
};
