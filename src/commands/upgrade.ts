/**
 * UpgradeLink SDK Integration
 * 使用 Tauri 官方 updater 配合 UpgradeLink 端点
 */

import { check, type Update } from "@tauri-apps/plugin-updater";

// 升级策略类型
export type UpgradeType = "force" | "prompt" | "silent" | null;

// 升级信息接口（仅内部使用）
interface UpdateInfo {
  upgradeType?: UpgradeType;
  rawJson?: {
    upgradeType?: number;
    [key: string]: unknown;
  };
}

// 更新检查结果
interface UpgradeCheckResult {
  hasUpdate: boolean;
  update: UpdateInfo | null;
  version: string | null;
  upgradeType: UpgradeType;
}

// UpgradeLink 应用密钥（从环境变量读取）
const UPGRADELINK_ACCESS_KEY = import.meta.env.VITE_UPGRADE_LINK_ACCESS_KEY ?? "";

/**
 * 解析 UpgradeLink 返回的 upgradeType
 * 0: 不升级, 1: 提示升级, 2: 强制升级, 3: 静默升级
 */
function parseUpgradeType(upgradeType: unknown): UpgradeType {
  switch (upgradeType) {
    case 2:
      return "force";
    case 3:
      return "silent";
    case 1:
      return "prompt";
    default:
      return "prompt"; // 默认提示升级
  }
}

/**
 * 检查更新（使用 Tauri 官方 updater + UpgradeLink 端点）
 * 
 * tauri.conf.json 中配置 UpgradeLink 端点：
 * "endpoints": [
 *   "https://api.upgrade.toolsetlink.com/v1/tauri/upgrade?tauriKey=xxx&versionName={{current_version}}&target={{target}}&arch={{arch}}"
 * ]
 */
export async function checkForUpdate(): Promise<UpgradeCheckResult> {
  try {
    // 使用 Tauri 官方 check()，它会调用 tauri.conf.json 中配置的 UpgradeLink 端点
    const update = await check({
      timeout: 10000,
      // 添加 UpgradeLink 认证头
      headers: {
        'X-AccessKey': UPGRADELINK_ACCESS_KEY,
      },
    });

    if (!update?.available) {
      return {
        hasUpdate: false,
        update: null,
        version: null,
        upgradeType: null,
      };
    }

    // 直接从 rawJson 中解析 upgradeType（UpgradeLink 返回的原始数据）
    const upgradeType = parseUpgradeType(update.rawJson?.upgradeType);

    return {
      hasUpdate: true,
      update: update as UpdateInfo,
      version: update.version,
      upgradeType,
    };
  } catch (error) {
    console.error("[upgrade] Failed to check update:", error);
    return {
      hasUpdate: false,
      update: null,
      version: null,
      upgradeType: null,
    };
  }
}

/**
 * 执行更新（桌面端）
 */
export async function executeDesktopUpdate(
  update: Update,
  onProgress?: (downloaded: number, total: number) => void,
): Promise<void> {
  let downloadedBytes = 0;
  let totalBytes = 0;

  await update.downloadAndInstall((event) => {
    if (!onProgress) return;

    switch (event.event) {
      case 'Started':
        totalBytes = event.data.contentLength || 0;
        break;
      case 'Progress':
        downloadedBytes += event.data.chunkLength;
        onProgress(downloadedBytes, totalBytes);
        break;
      case 'Finished':
        break;
    }
  });
}

