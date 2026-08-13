import {
  Laptop,
  type LucideIcon,
  Monitor,
  Smartphone,
  Tablet,
} from "lucide-react-native";

/**
 * OS 标识 → 设备图标。**与桌面 `getDeviceIcon` / Web `deviceIcon` 是同一份映射**：
 * windows / linux → `Monitor`，macos / darwin → `Laptop`，ios / android → `Smartphone`，
 * 识别不出一律 `Monitor`。
 *
 * 这里曾把 windows 也画成 `Laptop`，于是同一台 Windows 机器在手机上是笔记本、
 * 在桌面与浏览器里是显示器——设备卡的第一个信息位就对不上。
 *
 * `Tablet` 是本端独有的一档（另两端的映射表没有 ipad/tablet 的键），不算分叉：
 * 它落在两端共同的 fallback 之前，命中不了就还是 `Monitor`。
 */
export function devicePlatformIcon(platform: string): LucideIcon {
  const p = platform.toLowerCase();
  if (p.includes("ios") || p.includes("android")) return Smartphone;
  if (p.includes("ipad") || p.includes("tablet")) return Tablet;
  if (p.includes("mac") || p.includes("darwin")) return Laptop;
  return Monitor;
}
