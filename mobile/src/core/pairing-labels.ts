/**
 * 配对流程的判别码 → 文案映射（移动端这一份）。
 *
 * 三个入口（深链 `_layout` / 扫码 `pairing/scan` / 粘贴 `invite-exchange`）走的是同一条
 * `previewInvite`，被拒时也该说同一句话。此前三处各写了一遍同样的三元链，而
 * `network-labels.ts` 顶上那句教训（「三份 catalog 已经因为各写一份而漂过一次」）在这里
 * 同样成立 —— 只是还没漂而已。
 *
 * 存 `msg` 描述符而不是成品字符串：翻译宏只在组件里展开（见根 `CLAUDE.md`），
 * 调用点拿到描述符后用 `t(...)` 展开。
 */

import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import type { PreviewReject } from "@/stores/pairing-invite-store";

/**
 * 邀请预览被拒的三种分类各自的说法。
 *
 * 三条必须各说各的，不能合并成一句「邀请无效」：`self` 的正确动作是「让对方给你他的码」，
 * `expired` 是「请对方重新生成」，只有 `invalid` 才是「这串东西不对」。说成一样会让用户
 * 反复重新生成、重新扫，而问题可能只是他扫了自己那张码。
 *
 * `invalid` 这条允许调用点覆盖：扫码屏要说「对准另一台设备的二维码」，读剪贴板要说
 * 「剪贴板里的邀请无效」—— 同一个判别码，载体不同则下一步动作不同。
 */
export const PREVIEW_REJECT_MESSAGE: Record<PreviewReject, MessageDescriptor> =
  {
    self: msg`这是你自己的邀请`,
    expired: msg`邀请已过期，请让对方重新生成`,
    invalid: msg`邀请无效或已被使用`,
  };
