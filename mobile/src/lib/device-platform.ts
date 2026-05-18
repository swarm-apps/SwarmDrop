import {
  Laptop,
  type LucideIcon,
  Monitor,
  Smartphone,
  Tablet,
} from "lucide-react-native";

export function devicePlatformIcon(platform: string): LucideIcon {
  const p = platform.toLowerCase();
  if (p.includes("ios") || p.includes("android")) return Smartphone;
  if (p.includes("ipad") || p.includes("tablet")) return Tablet;
  if (p.includes("mac") || p.includes("windows")) return Laptop;
  return Monitor;
}
