import { commands, events } from "@/lib/bindings";
import { usePreferencesStore } from "@/stores/preferences-store";

/**
 * 设备名长度上限 —— Rust 侧 `DeviceName::MAX_CHARS` 的镜像，改上限要**同时改 Rust**
 * （`crates/host/src/device.rs`，那边是事实源，UI 这层只是提前拦一下）。
 *
 * 单位是 **char 不是 byte**：中文名可以起满 40 个字。`<input maxLength>` 按 UTF-16
 * code unit 计数，对 BMP 内字符（含全部常用汉字）与 Rust 的 char 计数一致。
 *
 * specta 不导出 const，所以这个值只能手抄一份，没法从 bindings 里拿。
 */
export const DEVICE_NAME_MAX_CHARS = 40;

/**
 * 设备显示名 —— 优先用用户设置的 name，缺省时回退到系统 hostname。
 *
 * 适用于 Device / PairedDeviceInfo / OsInfo / lookupDeviceByCode 返回值等所有
 * 含 `{ name?, hostname }` 形状的对象。
 */
export function deviceDisplayName(d: {
  name?: string | null;
  hostname: string;
}): string {
  return d.name?.trim() || d.hostname;
}

/**
 * 设置设备名 —— 写后端 device_config.json（事实源）+ 同步前端显示镜像。
 *
 * 后端在同一次调用里把新名字经 libp2p Identify 的 `agent_version` 逐连接下发并主动
 * push，已连接的对端一个 RTT 内就看到新名字：**节点不重启、连接不断、在途传输不中断**。
 * 此前那版由前端自己 `stopNetwork` + `startNetwork`，代价是断掉所有连接、打断正在传的
 * 文件，只为了让对端读到一个字符串。
 *
 * 传空串/纯空白清空，回退到系统 hostname。
 *
 * **失败一律 throw。** 后端把落盘放在编排的最前面，落盘失败即整体失败、一个字节都不推
 * 网络，所以没有「存住了但没广播出去」这种要分开汇报的中间态 —— 调用方接住异常提示用户
 * 即可，不必再判断节点状态。
 */
export async function applyDeviceName(name: string): Promise<void> {
  const trimmed = name.trim();
  // 返回的是后端归一化后的结果（剥了 `;` 与控制字符、按 char 截到 40），不是用户的
  // 原样输入 —— 镜像要显示对端真正会看到的那个名字。
  const stored = await commands.setDeviceName(trimmed || null);
  usePreferencesStore.setState({ deviceName: stored ?? "" });
}

let unlistenDeviceRenamed: (() => void) | null = null;

/**
 * 订阅 `device-renamed`（后端改名编排的最后一步发出）。
 *
 * 覆盖的是 [`applyDeviceName`] 够不到的来源：另一个窗口、MCP 工具。那些路径上本窗口的
 * `applyDeviceName` 压根没被调用过，镜像只能靠事件对齐。
 *
 * 反过来 `applyDeviceName` 自己那次同步也不能省：本监听器挂在 `_app` 布局下，而
 * onboarding 走的是 `_onboarding` 布局——首启那次改名发生时它还没挂。两条路写的是同一个
 * 值（都取后端归一化后的结果），重复覆盖无副作用。
 *
 * **挂应用生命周期而不是节点生命周期**：改名在节点停着时同样可用（后端走「只落盘」
 * 那条分支），挂进 network-store 的监听器会随 `stopNetwork` 一起注销。
 *
 * 写入 `deviceName` 的是 `name` 不是 `displayName` —— 前者为空表示「没设过、用
 * hostname」，把回退后的 hostname 存进去会让用户看起来「设过一个恰好等于机器名的名字」。
 */
export async function setupDeviceNameListener(): Promise<void> {
  cleanupDeviceNameListener();
  unlistenDeviceRenamed = await events.deviceRenamed.listen((event) => {
    usePreferencesStore.setState({ deviceName: event.payload.name ?? "" });
  });
}

export function cleanupDeviceNameListener(): void {
  unlistenDeviceRenamed?.();
  unlistenDeviceRenamed = null;
}

/**
 * 拉取后端持久化的设备名，覆盖前端缓存。应用启动 hydration 后调一次，确保
 * 跨设备/卸装重装时后端是 source of truth。
 */
export async function syncDeviceNameFromBackend(): Promise<void> {
  try {
    const backend = await commands.getDeviceName();
    if (backend !== null) {
      usePreferencesStore.setState({ deviceName: backend });
    }
  } catch (err) {
    console.warn("[device-name] sync from backend failed:", err);
  }
}
