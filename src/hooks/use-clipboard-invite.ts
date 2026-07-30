/**
 * 剪贴板感知（桌面）——「感知 + 一键确认」而非全自动（pair-invite-protocol design D7）。
 *
 * 窗口 focus 时读一次系统剪贴板，解码验签通过就把**对端身份**抬出来（返回给调用方亮一键
 * 条）。**不自动发起配对**——邀请是信任凭证，需用户点击才进确认卡（安全闸）。
 *
 * ## 三条与早期版本不同的地方（openspec: pair-deep-link design D4/D5）
 *
 * 1. **解码提前到检测阶段**。以前只做前缀字符串匹配、把裸串抬给调用方；现在直接调
 *    `decodePairInvite`，于是提示条能写出「张三的 MacBook 想和你配对」而不是干巴巴的
 *    「检测到配对邀请」。代价是每次 focus 多一次 IPC 往返 —— focus 不是高频事件，可以接受。
 * 2. **自我过滤**：邀请的 `peerId`（= wire 里的 `inviter_id`，在签名覆盖范围内）等于本机
 *    身份时静默忽略。用户复制自己刚生成的邀请准备发给别人，回到应用不该被问「要不要配对」。
 *    这个判据是**结构性**的，不需要「记住我复制过什么」那类状态，也伪造不了。
 * 3. **前缀硬编码没了**。判据交给后端的唯一解析入口，前端不再需要知道链接长什么样
 *    （早期这里有一份 `INVITE_URL_PREFIX` 的正则副本，改域名要两处一起改）。
 *
 * 读走 `@/lib/clipboard` 的原生 plugin 封装，不用 WebView 的 Clipboard API：后者在
 * WKWebView / WebKitGTK 上通常要求 transient activation，而 `focus` 事件不构成用户
 * 手势——曾长期靠 `catch` 吞掉失败，感知在哪些平台真正生效过无从判断。
 */

import { useCallback, useEffect, useState } from "react";

import { readText } from "@/lib/clipboard";
import { commands, type PairInvitePreview } from "@/lib/bindings";

/** 感知到的邀请：原文（供确认时消费）+ 已解码的对端身份（供提示条展示）。 */
export interface DetectedInvite {
  invite: string;
  preview: PairInvitePreview;
}

/**
 * 已处理过的剪贴板文本 —— 同一内容不重复解码、不重复亮条。
 *
 * **必须活得比组件长**：提示条挂在 `_app` 布局且对 `/pairing/*` 全屏路由条件渲染，而它
 * 自己点下去就是跳 `/pairing/input` —— 组件因此必然卸载一次。记忆若住在组件 state 里，
 * 回到列表页时就清零了，同一条邀请会反复弹；配对成功后导航回 `/devices` 更糟：
 * 提示「刚刚配好的那台设备想和你配对」，而 TTL 是 24 小时，这会持续一整天。
 *
 * 记原始文本而非解码结果：解码失败的内容也要记住，否则每次 focus 都白跑一次 IPC。
 * 进程级即可（App 重启后重新感知是合理的），故用 module 级而非 store。
 */
const seenClipboardTexts = new Set<string>();

/**
 * @param enabled 关闭时不读剪贴板（首启引导期、节点未启动等）
 * @param selfPeerId 本机身份；用于自我过滤，未知时传 null（此时不过滤）
 * @param pairedPeerIds 已配对设备的 peerId；已配对的不再提示（传 store 里的稳定引用）
 * @returns `detected` 感知到的邀请（未感知为 null）；`dismiss` 收起一键条
 */
export function useClipboardInvite(
  enabled: boolean,
  selfPeerId: string | null,
  pairedPeerIds: readonly string[],
) {
  const [detected, setDetected] = useState<DetectedInvite | null>(null);

  const check = useCallback(async () => {
    if (!enabled) return;
    let text: string;
    try {
      text = (await readText()).trim();
    } catch (error) {
      // 剪贴板为空、内容非文本、权限缺失都会走这里且不可区分（见 clipboard.ts）。
      // 静默降级——感知只是增强路径，不该打扰用户；但留一条 debug 痕迹，
      // 否则「这个功能在某平台从来没生效」永远不会被发现。
      console.debug("clipboard invite: read unavailable", error);
      return;
    }
    if (text === "" || seenClipboardTexts.has(text)) return;
    // 记在解码之前：解码失败的文本也算「处理过」，否则每次 focus 都白跑一次 IPC。
    seenClipboardTexts.add(text);

    // 交给唯一解析入口：它能从任意文本里定位邀请（IM 里连说明文字一起复制是常态），
    // 不是邀请就报错——这里正是「不是邀请」的正常路径，静默即可。
    let preview: PairInvitePreview;
    try {
      preview = await commands.decodePairInvite(text);
    } catch {
      return;
    }

    // 自我过滤：这是本机自己发出的邀请（用户复制它是为了发给别人）。
    if (selfPeerId !== null && preview.peerId === selfPeerId) return;
    // 已配对过滤：配完之后剪贴板里那条邀请还在，24h 内不该反复问「要不要和已配好的设备配对」。
    // 与自我过滤同构，都是结构性判据；而且它独立于「seen 记忆」生效——即使记忆因某种原因
    // 丢了，已配对的那条也不会再弹。
    if (pairedPeerIds.includes(preview.peerId)) return;

    setDetected({ invite: text, preview });
  }, [enabled, selfPeerId, pairedPeerIds]);

  useEffect(() => {
    if (!enabled) return;
    // 进入即检查一次 + 窗口重新聚焦时检查（用户复制后切回应用）
    void check();
    window.addEventListener("focus", check);
    return () => window.removeEventListener("focus", check);
  }, [enabled, check]);

  const dismiss = useCallback(() => setDetected(null), []);

  return { detected, dismiss };
}
