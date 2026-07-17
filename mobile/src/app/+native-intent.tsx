import { getShareExtensionKey } from "expo-share-intent";

/**
 * expo-router 入站 URL 拦截 —— expo-share-intent 的 Share Extension 会用
 * `swarmdrop://dataUrl=<key>?nonce=...` 拉起主 App。这个 URL **不是路由**,若不拦截
 * expo-router 会当成页面路径解析 → "Unmatched Route" 404。
 *
 * 这里识别分享 URL 后重定向到主入口(避免 404):分享数据本身由原生模块 keyed 保存,
 * 根布局的 `ShareIntentHandler`(useShareIntentContext)拿到 `hasShareIntent` 后再
 * push `/send/share-target`。其余 URL 原样放行(深链 `swarmdrop://` 正常路由)。
 *
 * 见 expo-share-intent 的 expo-router 集成说明。
 */
export function redirectSystemPath({
  path,
}: {
  path: string;
  initial: boolean;
}): string {
  try {
    if (path.includes(`dataUrl=${getShareExtensionKey()}`)) {
      return "/";
    }
    return path;
  } catch {
    return "/";
  }
}
