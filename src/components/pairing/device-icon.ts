/**
 * 设备图标工具
 * 根据操作系统名称返回对应的 Lucide 图标组件
 */

import type { ComponentType } from "react";
import { Monitor, Smartphone, Laptop } from "lucide-react";

const deviceIcons: Record<string, ComponentType<{ className?: string }>> = {
  windows: Monitor,
  linux: Monitor,
  macos: Laptop,
  darwin: Laptop,
  ios: Smartphone,
  android: Smartphone,
};

/**
 * 根据设备 OS 标识返回对应的 Lucide 图标组件。
 *
 * 入参是「设备的 OS 标识字符串」——调用点里它可能叫 `device.os` / `osInfo.os` /
 * `platform` / `currentOsType`，都是同义的同一个值。内部统一小写后匹配
 * （windows/linux → Monitor，macos/darwin → Laptop，ios/android → Smartphone），
 * 无法识别时回退到 Monitor。
 */
export function getDeviceIcon(os: string) {
  return deviceIcons[os.toLowerCase()] ?? Monitor;
}
