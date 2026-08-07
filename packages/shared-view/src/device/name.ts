/**
 * 设备名的跨端共用规则。
 *
 * 「怎么算出要显示的那个名字」三端必须一致，否则同一台设备在桌面叫 hostname、在手机上
 * 叫别的，用户会以为是两台。改名**动作**本身各端不同（桌面写 device_config + 同步 store
 * 镜像、移动走 uniffi、Web 一次 wasm 调用），留在各端。
 */

/** 任何含 `{ name?, hostname }` 形状的设备快照——三端各自的 `Device` 都能结构化赋值。 */
export interface DeviceNameSource {
  name?: string | null;
  hostname: string;
}

/**
 * 设备名长度上限 —— Rust 侧 `DeviceName::MAX_CHARS` 的镜像。事实源在
 * `crates/host/src/device.rs`，UI 这层只是提前拦一下；改上限要**同时改 Rust**。
 *
 * 单位是 **char 不是 byte**：中文名可以起满 40 个字。`<input maxLength>` 与 RN
 * `TextInput` 的 `maxLength` 都按 UTF-16 code unit 计数，对 BMP 内字符（含全部常用汉字）
 * 与 Rust 的 char 计数一致。
 *
 * specta / wasm-bindgen / uniffi 三条 codegen 都不导出常量，所以它只能手抄——
 * 但**三端抄同一份**：这里。
 */
export const DEVICE_NAME_MAX_CHARS = 40;

/**
 * 设备显示名 —— 优先用用户设置的 name，缺省时回退到系统 hostname。
 *
 * 空串与纯空白都算「没设」：内核允许传空串表示清空，之后 `name` 是 `Some("")` 还是
 * `None` 取决于路径，UI 不该被这个差别绊到。
 */
export function deviceDisplayName(device: DeviceNameSource): string {
  return device.name?.trim() || device.hostname;
}
