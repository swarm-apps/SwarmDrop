/**
 * 前端发起的进程终止的**唯一入口**（退出 / 重启更新）。
 *
 * 终止进程前必须先把偏好写入送到后端：`quitApp` / `relaunch` 都是同步命令，Rust 侧当场
 * 结束进程，而偏好写入是一次还在路上的 IPC——不先 flush 就会把它连同进程一起丢掉。
 *
 * **判据是时间尺度，不是「附近有没有写偏好」。** 托盘「退出」（Rust 侧 `app.exit(0)`）与
 * macOS `Cmd+Q` 不走这里，但它们同样紧邻着偏好写入（比如刚在设置页改完就按 Cmd+Q）；
 * 区别在于那中间隔着**一次人类操作**（≥100ms 量级，而 IPC 是 ~1ms），是概率上输不了。
 * 这里这两条是 `setCloseBehavior(...)` 之后**同一个 tick 程序化地**终止进程，必输。
 *
 * 所以新增终止路径时该问的是「中间隔着人吗」，而不是「附近有没有 set」。
 * `scripts/check-quit-entry.mjs` 守着这条约束——绕过本模块直接 invoke 会编不过。
 */

import { relaunch } from "@tauri-apps/plugin-process";
import { commands } from "@/lib/bindings";
import { flushTauriStores } from "@/lib/tauri-store";

/** 退出应用。 */
export async function quitApp(): Promise<void> {
  await flushTauriStores();
  await commands.quitApp();
}

/** 重启应用（更新装好后让新版本生效）。 */
export async function relaunchApp(): Promise<void> {
  await flushTauriStores();
  await relaunch();
}
