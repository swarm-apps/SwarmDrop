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
import type { TrustLevel } from "@swarmdrop/shared-view";
import type { Device } from "./view-types";

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
