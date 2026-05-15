/**
 * Desktop Updater Service
 *
 * 把 Tauri updater 的 check / download / install 包装成可注入的服务，
 * 让 [`upgrade-link-store`](../stores/upgrade-link-store.ts) 只关心状态字段，
 * 不必再关心进度限速 / 速度计算的实现细节。
 *
 * UpgradeLink 端点和升级策略解析仍走 [`commands/upgrade`](../commands/upgrade.ts)。
 */

import {
  check as checkDesktopUpdate,
  type Update,
} from "@tauri-apps/plugin-updater";
import { executeDesktopUpdate } from "@/commands/upgrade";

/** 下载进度回调每次接收到 chunk 时的当前快照。 */
export interface DownloadSnapshot {
  downloaded: number;
  total: number;
  /** 字节/秒（500ms 滑动窗口估算） */
  speed: number;
  /** 0–100 整数百分比 */
  percent: number;
}

/** 用 500ms 窗口节流出"下载速度"。store 不必自己维护。 */
class DownloadSpeedTracker {
  private lastDownloaded = 0;
  private lastTick = Date.now();
  private currentSpeed = 0;
  private totalSize = 0;

  setTotal(total: number) {
    if (total > 0) this.totalSize = total;
  }

  snapshot(downloaded: number): DownloadSnapshot {
    const now = Date.now();
    if (now - this.lastTick > 500) {
      const elapsed = (now - this.lastTick) / 1000;
      this.currentSpeed = elapsed > 0 ? (downloaded - this.lastDownloaded) / elapsed : 0;
      this.lastDownloaded = downloaded;
      this.lastTick = now;
    }
    const percent = this.totalSize > 0 ? Math.round((downloaded / this.totalSize) * 100) : 0;
    return {
      downloaded,
      total: this.totalSize,
      speed: this.currentSpeed,
      percent,
    };
  }
}

/** 用 Tauri updater + UpgradeLink 头检查桌面端更新；返回 `Update` 句柄供后续 `runUpdate`。 */
export async function fetchDesktopUpdate(timeoutMs = 10000): Promise<Update | null> {
  const update = await checkDesktopUpdate({ timeout: timeoutMs });
  return update?.available ? update : null;
}

/** 串起 download + install + 进度限速；onProgress 接收节流后的 snapshot。 */
export async function runDesktopUpdate(
  update: Update,
  onProgress: (snapshot: DownloadSnapshot) => void,
): Promise<void> {
  const tracker = new DownloadSpeedTracker();
  await executeDesktopUpdate(update, (downloaded, total) => {
    if (total !== undefined) tracker.setTotal(total);
    onProgress(tracker.snapshot(downloaded));
  });
}
