/**
 * canonical 邀请链接的前端判据 —— 移动端**唯一**的一份前缀副本。
 *
 * **两个前缀都受理**（与 Rust 的 `ACCEPTED_URL_PREFIXES` 对齐）：站点当前在 GitHub Pages
 * 子路径上，`swarmapp.cn` 待备案。两个都认，所以将来切换主域名**不需要再改这里**。
 *
 * 权威定义在 Rust 的 `INVITE_URL_PREFIX`（`crates/invite/src/invite.rs`）。这里是副本，
 * 因为深链拦截（`+native-intent.tsx`）跑在 React 之前、拿不到原生模块，扫码器也需要在
 * 不进 core 的前提下静默滤掉无关二维码。改域名要同步的完整清单见那个常量的文档注释。
 *
 * **判据带上字符集与长度下限**：只比前缀会把任何同域链接都送进 `previewInvite`，
 * 白白锁住扫码器再弹一次「邀请无效」。
 */
const INVITE_LINK_SOURCE = String.raw`https://(?:swarm-apps\.github\.io/SwarmDrop|swarmapp\.cn)/p/#[A-Za-z2-7]{32,}`;

/**
 * 锚定匹配：二维码内容应当是干净的一整串。
 *
 * 无 `g` flag，所以 `test` / `exec` 不带 lastIndex 状态，可以安全地当模块级常量共享。
 */
export const INVITE_PATTERN = new RegExp(`^${INVITE_LINK_SOURCE}$`, "i");

/**
 * 文本内搜索：从微信 / 邮件复制常常连着说明文字，深链也把整条链接挂在 `swarmdrop:`
 * 后面 —— 与 core 的 `PairInvite::decode` 行为一致（它也是在任意文本里定位前缀）。
 */
export const INVITE_IN_TEXT_PATTERN = new RegExp(INVITE_LINK_SOURCE, "i");

/** 从任意文本里提取邀请链接；没有则 `null`。 */
export function extractInviteLink(text: string): string | null {
  return INVITE_IN_TEXT_PATTERN.exec(text)?.[0] ?? null;
}
