import * as Device from "expo-device";
import { initMobileCore } from "@/core/mobile-core";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 设备名长度上限 —— Rust 侧 `DeviceName::MAX_CHARS` 的镜像，改上限要**同时改 Rust**
 * （`crates/host/src/device.rs`，那边是事实源，UI 这层只是提前拦一下）。
 *
 * 单位是 **char 不是 byte**：中文名可以起满 40 个字。RN `TextInput` 的 `maxLength`
 * 按 UTF-16 code unit 计数，对 BMP 内字符（含全部常用汉字）与 Rust 的 char 计数一致。
 *
 * uniffi 不导出 Rust 的关联常量，所以这个值只能手抄一份，没法从 generated 里拿。
 */
export const DEVICE_NAME_MAX_CHARS = 40;

/**
 * 设备显示名 —— 优先用用户起的 name，缺省时回退到系统 hostname。
 *
 * 适用于 MobileDevice / MobileRemoteDeviceInfo / 任何含 `{ name?, hostname }`
 * 形状的对象。
 */
export function deviceDisplayName(d: {
  name?: string | null;
  hostname: string;
}): string {
  return d.name?.trim() || d.hostname;
}

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
