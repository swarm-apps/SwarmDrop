import { getShareExtensionKey } from "expo-share-intent";

import { extractInviteLink } from "@/core/invite-link";
import { stashPendingInvite } from "@/core/pending-deep-link";

/**
 * expo-router 入站 URL 拦截 —— 本 App **唯一**的 URL 分发口。
 *
 * 两类非路由 URL 在这里被认走，其余原样放行：
 *
 * | 入站 URL | 处理 |
 * |---|---|
 * | `swarmdrop://dataUrl=<key>?nonce=…`（iOS Share Extension） | 重定向 `/`，数据由原生模块 keyed 保存，`ShareIntentHandler` 再 push |
 * | `swarmdrop:https://swarm-apps.github.io/SwarmDrop/p/#<payload>`（配对深链） | 邀请放进单槽，重定向 `/`，`DeepLinkInviteHandler` 再走确认卡 |
 *
 * 不拦截的话 expo-router 会把它们当成页面路径解析 → "Unmatched Route" 404
 * （iOS 模拟器 Maestro E2E 实测抓到的坑）。
 *
 * **为什么深链也走这里，而不是在根布局加 `Linking.addEventListener`**：那会造出第二个
 * URL 消费者，与本函数竞争同一条 URL（谁先拿到取决于 expo-router 内部时序），分享 URL
 * 也会一并流进那个监听器。URL 分发只留一个口，是这套里最要紧的一条约束。
 *
 * 本函数跑在 React 挂载之前，所以**不能** push 路由、不能碰原生模块 —— 只能放下负载 +
 * 返回一个路径，由树里的 handler 接手（见 `@/core/pending-deep-link`）。
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
    // 深链把整条 canonical 链接挂在 `swarmdrop:` 后面（**单冒号**形态，契约与理由见
    // `src-tauri/src/external_open.rs` 的 `tests::deep_link_contract`），所以这里在
    // **任意文本**里定位邀请，而不是假设 URL 的结构 —— 与 core 的 decode 同一策略。
    const invite = extractInviteLink(path);
    if (invite !== null) {
      stashPendingInvite(invite);
      return "/";
    }
    return path;
  } catch {
    return "/";
  }
}
