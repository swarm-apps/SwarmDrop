import * as Device from "expo-device";
import { initMobileCore } from "@/core/mobile-core";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 移动端设备名模块。**纯规则（显示名、长度上限）已收口到 `@swarmdrop/shared-view`**，
 * 这里只留移动端特有的名字建议与改名编排，并把共享规则原样再导出——调用方仍从
 * `@/lib/device-name` 一处拿全套。
 */
export {
  DEVICE_NAME_MAX_CHARS,
  type DeviceNameSource,
  deviceDisplayName,
} from "@swarmdrop/shared-view";

/**
 * 给 onboarding 输入框的默认值 —— 优先 expo-device 的 deviceName（Android 一般
 * 拿得到用户起的蓝牙/设备名；iOS 16+ 上多半是 "iPhone" 字符串），缺省时回退到
 * modelName（"iPhone 15 Pro" / "Pixel 8"）。最差用调用方传入的本地化兜底
 * （必填,由调用方负责本地化——lib 层保持 i18n-free）。
 */
export function suggestedDeviceName(fallbackName: string): string {
  return Device.deviceName?.trim() || Device.modelName?.trim() || fallbackName;
}

/**
 * 设置设备名 —— 一次调用 core 的 `renameDevice`，落盘 + 让已连接的对端立刻看到。
 *
 * 传空串/纯空白清空，回退到系统 hostname。
 *
 * **不再重启节点。** core 那条编排（写盘 → 更新本机 OsInfo → 向已连接对端推
 * Identify `agent_version` → 广播改名事件）在一个 RTT 内就让对端更新，连接、relay
 * reservation 与进行中的传输全都不受影响。节点没在跑（onboarding、或设置页早于
 * `startNode`）时它只落盘，同样返回成功 —— 分支在 core 里，这里不判断。
 *
 * 失败（落盘失败 / 推送失败）会 throw，调用方据此提示用户。
 */
export async function applyDeviceName(name: string): Promise<void> {
  const trimmed = name.trim();
  const core = await initMobileCore();
  // uniffi 把 Rust 的 `Option<String>` 映射成 `string | undefined`（不是 null）。
  // 返回的是 core 归一化后的结果（剥了 `;` 与控制字符、按 char 截到 40），不是用户
  // 的原样输入 —— 镜像要显示对端真正会看到的那个名字。
  const stored = await core.renameDevice(trimmed || undefined);
  // 镜像只在 core 成功之后写：反过来先写镜像，失败时 UI 显示的是新名字，对端与下次
  // 启动看到的却是旧的。
  usePreferencesStore.getState().setDeviceName(stored ?? "");
}
