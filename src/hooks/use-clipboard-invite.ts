/**
 * 剪贴板感知（桌面）——「感知 + 一键确认」而非全自动（pair-invite-protocol design D7）。
 *
 * 桌面 Tauri webview 读剪贴板无系统提示：窗口 focus 时静默读一次，命中 `sdinvite` 前缀
 * 就把邀请串抬出来（返回给调用方亮一键条）。**不自动发起配对**——邀请是信任凭证，
 * 需用户点击确认才 previewInvite（安全闸）。同一串只提示一次（避免反复弹）。
 */

import { useCallback, useEffect, useState } from "react";

/** 邀请串前缀（裸 base32 形态；深链形态后续 change 再加） */
const INVITE_PREFIX = "sdinvite";

/**
 * @returns `detected` 感知到的邀请串（未感知为 null）；`dismiss` 忽略本次；`clear` 消费后清空
 */
export function useClipboardInvite(enabled: boolean) {
  const [detected, setDetected] = useState<string | null>(null);
  // 已提示过的串——同一内容不重复亮条
  const [seen, setSeen] = useState<string | null>(null);

  const check = useCallback(async () => {
    if (!enabled) return;
    let text: string;
    try {
      text = (await navigator.clipboard.readText()).trim();
    } catch {
      return; // 读失败（无权限/空）静默忽略
    }
    if (!text.startsWith(INVITE_PREFIX)) return;
    if (text === seen) return; // 已提示过，不重复
    setDetected(text);
    setSeen(text);
  }, [enabled, seen]);

  useEffect(() => {
    if (!enabled) return;
    // 进入即检查一次 + 窗口重新聚焦时检查（用户复制后切回应用）
    void check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [enabled, check]);

  const dismiss = useCallback(() => setDetected(null), []);
  const clear = useCallback(() => setDetected(null), []);

  return { detected, dismiss, clear };
}
