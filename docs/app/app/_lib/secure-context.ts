// secure-context 门：非 secure 源（http 私网 IP）下浏览器不提供 `navigator.storage`(OPFS) 与
// `crypto.subtle`，接收方 finalize 落盘会静默挂死（web-sys 的 JsFuture 打到 undefined 永久
// pending）。启动即检测并显式预警，不等传到一半才发现。详见 dev-notes/knowledge/libp2p-wasm.md
// 的「页面必须是 HTTPS」与「五道运行时门·门 4」。
//
// secure context 白名单：https://* / http://localhost / http://127.0.0.1 / file://。
// 私网 IP over http 不在内。

export interface SecureContextInfo {
  /** window.isSecureContext */
  isSecure: boolean;
  /** OPFS 可用（navigator.storage.getDirectory 存在） */
  hasStorage: boolean;
  /** WebCrypto 可用（crypto.subtle 存在） */
  hasSubtle: boolean;
}

/**
 * 检测当前环境。SSR / 静态导出预渲染（无 window）时返回乐观默认，客户端 effect 会立即校正
 * ——横幅只在客户端拿到真值后才可能出现，不会在预渲染 HTML 里闪一个错误的警告。
 */
export function detectSecureContext(): SecureContextInfo {
  if (typeof window === "undefined") {
    return { isSecure: true, hasStorage: true, hasSubtle: true };
  }
  return {
    isSecure: window.isSecureContext,
    hasStorage:
      typeof navigator !== "undefined" &&
      typeof navigator.storage?.getDirectory === "function",
    hasSubtle: typeof crypto !== "undefined" && !!crypto.subtle,
  };
}

/** 三查全过才能可靠接收落盘。任一缺失 → 接收方 finalize 会失败，需给出预警。 */
export function isReceiveCapable(info: SecureContextInfo): boolean {
  return info.isSecure && info.hasStorage && info.hasSubtle;
}
