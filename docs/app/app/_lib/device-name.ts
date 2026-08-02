// 设备名的两条共用规则。与桌面 `src/lib/device-name.ts`、移动 `mobile/src/lib/device-name.ts`
// 同名同职责——三端各有一份，因为三个 workspace 之间没法共享 TS 常量。
//
// 这里**只放不依赖运行时的纯规则**。改名动作本身在 node-panel 里直接调 `WebNode.rename_device`
// （Web 侧没有桌面那样的「写配置 + 同步 store 镜像」两步编排，wasm 导出一次调用就完事）。

import type { Device } from "./view-types";

/**
 * 设备名长度上限 —— Rust 侧 `DeviceName::MAX_CHARS` 的镜像（`crates/host/src/device.rs`，
 * 那边是事实源，UI 这层只是提前拦一下）。改上限要**同时改 Rust**。
 *
 * 单位是 **char 不是 byte**：中文名可以起满 40 个字。`<input maxLength>` 按 UTF-16
 * code unit 计数，对 BMP 内字符（含全部常用汉字）与 Rust 的 char 计数一致。
 *
 * wasm-bindgen 与 specta 都不导出 const，所以三端各手抄一份，没法从 bindings 里拿。
 * 桌面端 `DEVICE_NAME_MAX_CHARS` 的注释里记的是同一件事。
 */
export const DEVICE_NAME_MAX_CHARS = 40;

/**
 * 设备显示名 —— 优先用用户设置的 name，缺省时回退到 hostname。
 *
 * 与桌面端同名函数同语义（`name?.trim() || hostname`）：空串与纯空白都算「没设」，
 * 因为内核允许传空串表示清空，那之后 `name` 是 `Some("")` 还是 `None` 取决于路径，
 * UI 不该被这个差别绊到。
 */
export function deviceDisplayName(d: Pick<Device, "name" | "hostname">): string {
  return d.name?.trim() || d.hostname;
}
