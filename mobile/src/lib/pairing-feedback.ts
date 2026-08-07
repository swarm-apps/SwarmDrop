import { t } from "@lingui/core/macro";

import { toast } from "@/lib/toast";

/**
 * 配对达成后，如果这条记录没能落盘就如实告诉用户。
 *
 * `persisted === false` 是一种**「一半成功」**：对端已经收到 `Success` 并把本机写进了它的
 * 已配对列表，本次运行内这台设备也能正常用 —— 只是记录没写进 keychain，重启后会不见。
 *
 * 它既不能报成失败（对方明明成功了，报错会让两台设备的认知永久分叉、且没有任何一端会去
 * 纠正），也不能当成普通成功（用户不知道自己还得再配一次）。所以照实说。
 *
 * 三个配对入口（近场直连 / 消费邀请 / 响应入站请求）共用这一条，文案不要各写各的。
 * 判据与返回值的形态见 core 的 `PairedDeviceCommit`。
 */
export function warnIfPairingNotPersisted(persisted: boolean) {
  if (persisted) return;
  // **`info` 不是随手选的**：`@/lib/toast` 的门面只有 success / info / error 三档
  // （底层 burnt 的 preset），没有 warning 级别 —— 桌面与 Web 那两端用的是
  // `toast.warning`。这里退化成中性提示，所以带成功页的两条路径（近场直连 / 消费邀请）
  // 还会在 `pairing/success.tsx` 上内联一行 caveat，不能只靠这条 toast。
  toast.info(t`配对成功，但这条记录没能保存`, {
    description: t`重启应用后需要重新配对。`,
  });
}
