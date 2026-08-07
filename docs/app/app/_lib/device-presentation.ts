// 设备卡片的**呈现映射**：把 `Device` 上的枚举值翻成图标与配色。
//
// 为什么这些不在 `@swarmdrop/shared-view`：图标组件与 Tailwind class 都是平台的
// （移动端是 `lucide-react-native` + NativeWind，桌面另有一套玻璃拟态 class），
// 收进共享包就得把三端的 UI 依赖一起带进去。共享包只管**判定**（信任级别归一、能否发送），
// 呈现留在各端——这条分工写在该包的 README「归属判据」里。
//
// 文案也不在这里：徽标的字要翻译，而翻译宏只在组件里展开。本模块只返回图标与 class，
// 文案由 `device-card.tsx` 按同一个枚举值 switch 出 `<Trans>`。

import { Laptop, Monitor, RadioTower, Smartphone, Wifi, Zap, type LucideIcon } from "lucide-react";
import { shortPeerId, type TrustLevel } from "@swarmdrop/shared-view";
import type { Device } from "./view-types";

/**
 * 一条**记录 / 请求**上「对方是谁」那一格的显示名。
 *
 * 与 `organizedDeviceName` 的分工：那个用于**设备清单**，读的是当下的 `Device`，还会算上
 * 本机别名；这个读的是**落库那一刻的名字快照**（传输记录的 `peerName`、收件箱的
 * `sourceName`、入站 offer 的 `deviceName`），它们都可能是空串——内核的
 * `OsInfo::display_name()` 在 name 与 hostname 都没有时就返回空，占位归展示层。
 *
 * 空着渲染的后果按位置递增：列表里那一格不再回答「发给谁」；收件箱是永久表，空名会一直留着；
 * 而入站 offer 的确认框会变成「『』想发送 3 个文件」——请用户对一个无名氏的传输做决定。
 *
 * 回落用设备页 `organizedDeviceName` 最后一档的**同一个** `shortPeerId`，不另造截断规则；
 * 不用「未知设备」这类文案——那会让两台无名设备显示成同一行字，比空白更糟。
 *
 * `name` 收 `null | undefined` 是因为跨 wasm 边界的形状类型层保证不了
 * （知识库：`.d.ts` 说 `string`，运行时可能是别的）。这一格现在有五个渲染点共用，
 * 让其中任何一个因为 `undefined.trim()` 把整页打白不值当。
 *
 * ⚠️ **产生空值的是共享内核**，桌面（`-session-row.tsx` / `session-panel.tsx`）与移动
 * （`history-transfer-row.tsx` 等）的接收方向记录有同一个空值面，目前都是裸渲染。补齐它们时
 * 这个函数该上移 `@swarmdrop/shared-view`（现在只有一端用，按该包 README 的归属判据 2 不进）。
 */
export function peerLabel(name: string | null | undefined, peerId: string): string {
  return name?.trim() || shortPeerId(peerId);
}

const OS_ICONS: Record<string, LucideIcon> = {
  windows: Monitor,
  linux: Monitor,
  macos: Laptop,
  darwin: Laptop,
  ios: Smartphone,
  android: Smartphone,
};

/** OS 标识 → 图标。识别不出的一律 `Monitor`，与桌面 `getDeviceIcon` 同一份映射。 */
export function deviceIcon(os: string): LucideIcon {
  return OS_ICONS[os.toLowerCase()] ?? Monitor;
}

/** 连接方式（局域网 / 打洞 / 中继）的图标与配色。`null` 连接方式没有徽标。 */
export const CONNECTION_META: Record<
  NonNullable<Device["connection"]>,
  { Icon: LucideIcon; className: string }
> = {
  lan: { Icon: Wifi, className: "bg-success/12 text-success-ink" },
  dcutr: { Icon: Zap, className: "bg-info/12 text-info-ink" },
  relay: { Icon: RadioTower, className: "bg-warning/15 text-warning-ink" },
};

/**
 * 信任级别的配色。**只有 blocked 用 destructive**——其余三级是中性到正向的梯度，
 * 给它们上强色会让「协作者」看起来像个警告。
 */
export const TRUST_META: Record<TrustLevel, { className: string }> = {
  owned: { className: "bg-[var(--brand-solid)]/12 text-brand" },
  collaborator: { className: "bg-muted text-muted-foreground" },
  temporary: { className: "bg-warning/15 text-warning-ink" },
  blocked: { className: "bg-destructive/12 text-destructive-ink" },
};
