// 邀请相关的前端常量与推导。**Web 端前缀副本只有这一份**——组件里不要再写字面量。

import { msg } from "@lingui/core/macro";
import type { MessageDescriptor } from "@lingui/core";
import { formatTimeLeft } from "@swarmdrop/shared-view";

/**
 * 邀请有效期（秒），与 `crates/invite/src/invite.rs` 的 `INVITE_TTL_SECS` 一致。
 * **改这里必须同步那个 Rust 常量**——桌面端 `src/stores/pairing-store.ts` 与移动端
 * `mobile/src/stores/pairing-invite-store.ts` 各自也留了一份**同名**副本，同样的约定。
 *
 * 同名同单位是刻意的：这类跨语言复制唯一的兜底就是 `grep INVITE_TTL_SECS` 能一次捞齐
 * 所有副本，起个别名等于把自己从那次 grep 里摘出去。
 */
export const INVITE_TTL_SECS = 86_400;

/** 文案用的小时数，从上面派生——别在句子里直接写「24」，TTL 一改那些数字就开始骗人。 */
export const INVITE_TTL_HOURS = INVITE_TTL_SECS / 3600;

/**
 * canonical 邀请链接的主前缀，对齐 Rust 的 `INVITE_URL_PREFIX`（同名，理由同上）。
 *
 * **落地页兜底路径拼链接时必须用它，不能用 `location.origin`**：解码器只认受理列表里的
 * 前缀，别的域名一律拒（`invite.rs` 里 `https://evil.example/P/#…` 那条负例钉着），
 * 而站点跑在预览域名或 localhost 时 `location.origin` 拼出来的串会被拒掉。
 */
export const INVITE_URL_PREFIX = "https://swarm-apps.github.io/SwarmDrop/p/#";

/**
 * 邀请链接的判据，对齐 Rust 的 `ACCEPTED_URL_PREFIXES` 与移动端的 `INVITE_LINK_SOURCE`。
 *
 * **两个域名都受理**：迁移期两种链接会同时在外面飘（邀请活 24h，客户端也不会同一秒更新），
 * 只认主前缀等于迁移当天所有在途邀请一起失效。
 *
 * **判据带字符集与长度下限**：只比前缀会把任何同域链接都当成邀请。
 */
const INVITE_LINK_SOURCE = String.raw`https://(?:swarm-apps\.github\.io/SwarmDrop|swarmapp\.cn)/p/#[A-Za-z2-7]{32,}`;

/**
 * 文本内搜索，**不锚定行首、大小写不敏感**——这两点都是必须的，且都吃过亏才知道：
 *
 * - 从微信 / 邮件复制常常连着说明文字（「配对链接：https://…」），锚定行首就整个不认。
 *   后端 `PairInvite::decode` 本来就是在任意文本里定位前缀的，前端判据比它严，
 *   表现为「手动粘进输入框能配对，剪贴板感知却装看不见」。
 * - 部分客户端会把链接首字母大写或整段归一化，后端用的是 `to_ascii_lowercase` 比较。
 *
 * 无 `g` flag，故 `exec` 不带 lastIndex 状态，可安全地当模块级常量共享。
 */
const INVITE_IN_TEXT_PATTERN = new RegExp(INVITE_LINK_SOURCE, "i");

/**
 * 从任意文本里提取邀请链接；没有则 `null`。
 *
 * 只是筛，不是校验——真正的受理与验签在 Rust 侧。**前缀从来不是信任边界，签名才是。**
 */
export function extractInviteLink(text: string): string | null {
  return INVITE_IN_TEXT_PATTERN.exec(text)?.[0] ?? null;
}

/**
 * 邀请剩余秒数。`expiresAt` 是字符串承载的 Unix 秒（wasm 侧避开 u64 → BigInt 的取回麻烦），
 * `now` 由调用方传入（见 `use-now-seconds.ts`）——这样同一帧里的多处显示不会各读各的时钟。
 */
export function remainingSeconds(expiresAt: string, now: number): number {
  return Number(expiresAt) - now;
}

/**
 * 剩余有效期文案的**描述符 + 参数**。本模块不是组件，翻译宏在这里只能定义、不能展开。
 *
 * 已过期的条目本不该出现在注册表里（后端按 now 过滤），但时钟跳变或列表未刷新时会撞上，
 * 所以显式给一句而不是显示负数。
 *
 * 时长用 [`formatTimeLeft`] 而不是通用的 `formatDuration`：邀请 TTL 是 24 小时，紧凑形式会
 * 渲染成「23h 59m 后失效」这种中英混排；`formatTimeLeft` 走 `Intl` 的 unit 样式，按量级切粒度
 * （小时级只说小时），与桌面端邀请卡的说法一致。它需要一个 locale，故由调用点传入。
 */
export function remainingLabel(expiresAt: string, now: number, locale: string): MessageDescriptor {
  const seconds = remainingSeconds(expiresAt, now);
  if (seconds <= 0) return msg`已过期`;
  const left = formatTimeLeft(seconds, locale);
  return msg`${left}后失效`;
}
