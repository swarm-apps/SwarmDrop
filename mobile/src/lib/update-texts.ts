import {
  DEFAULT_LANGUAGE,
  isSupportedLanguage,
  type SupportedLanguage,
} from "@/i18n/languageDetector";
import { i18n } from "@/i18n/lingui";
import type { ApkInstallBlockReason } from "@/lib/ports";
import { readyHintKind } from "@/lib/update-dialog-visibility";

export type UpdateLocale = SupportedLanguage;

export interface UpdateTexts {
  /** 提示弹窗标题。 */
  promptTitle: string;
  /** 提示弹窗描述:(新版本, 当前版本) => 文案。 */
  promptDescription: (latest: string, current: string) => string;
  /** release notes 区块标题。 */
  releaseNotesLabel: string;
  /** "稍后提醒"按钮。 */
  laterButton: string;
  /** "立即更新"按钮。 */
  updateButton: string;
  /** 下载中按钮。 */
  downloadingButton: string;
  /** 强制更新标题。 */
  forceTitle: string;
  /** 强制更新描述:(新版本, 当前版本) => 文案。 */
  forceDescription: (latest: string, current: string) => string;
  /** 进度弹窗标题。 */
  progressTitle: string;
  /** 设置区块标题。 */
  settingsTitle: string;
  /** "检查更新"按钮。 */
  checkButton: string;
  /** 检查中。 */
  checkingButton: string;
  /** 已是最新。 */
  upToDate: string;
  /** 发现新版本:(新版本) => 文案。 */
  updateAvailable: (latest: string) => string;
  /** 当前版本标签:(当前版本) => 文案。 */
  currentVersionLabel: (current: string) => string;
  /** 检查失败。 */
  checkFailed: string;
  /** 重试按钮。 */
  retryButton: string;

  // —— RN 专属(native APK 安装层)——
  // Tauri 端用不到这些键;它们在 RN 的「下载完成 → 系统安装器接管」语义里出现。
  /** ready 态主按钮:点击拉起系统安装器。 */
  installButton: string;
  /**
   * ready 态提示。**陈述本地事实 + 给出动作**,不要陈述系统那边的状态 —— app 无从观测
   * 系统安装框到底弹没弹,而在后台被拦的场景里它确实没弹。
   */
  readyHint: string;
  /** 自动尝试已用掉、仍停在 ready 时的温和提示(多半是用户在系统框点了取消)。 */
  canceledRetry: string;
  /** app 不在前台、安装 intent 未派发时的提示(install 门禁 reason = background)。 */
  foregroundRequiredHint: string;
  /** 进度弹窗的退出按钮:只收起 UI,下载继续。 */
  backgroundButton: string;
}

/**
 * ready 态该显示哪句提示。判据在 update-dialog-visibility 的 `readyHintKind`(纯函数、可测),
 * 这里做「判据 → 文案」的映射并把两者合起来 —— 四个组件否则要各写一遍同样的组合。
 */
export function readyHintText(
  t: UpdateTexts,
  blockedReason: ApkInstallBlockReason | null,
  autoAttemptSpent: boolean,
): string {
  switch (readyHintKind(blockedReason, autoAttemptSpent)) {
    case "background":
      return t.foregroundRequiredHint;
    case "canceled":
      return t.canceledRetry;
    default:
      return t.readyHint;
  }
}

const en: UpdateTexts = {
  promptTitle: "Update available",
  promptDescription: (latest, current) =>
    `Version ${latest} is available (current ${current}).`,
  releaseNotesLabel: "What's new",
  laterButton: "Later",
  updateButton: "Update now",
  downloadingButton: "Downloading…",
  forceTitle: "Update required",
  forceDescription: (latest, current) =>
    `Version ${current} is no longer supported. Please update to ${latest}.`,
  progressTitle: "Downloading update",
  settingsTitle: "Software update",
  checkButton: "Check for updates",
  checkingButton: "Checking…",
  upToDate: "You're on the latest version.",
  updateAvailable: (latest) => `Version ${latest} is available.`,
  currentVersionLabel: (current) => `Current version ${current}`,
  checkFailed: "Update check failed.",
  retryButton: "Retry",

  installButton: "Install",
  readyHint: "Update ready — tap to install",
  canceledRetry: "Installation canceled. You can try again.",
  foregroundRequiredHint: "Reopen the app to finish installing.",
  backgroundButton: "Continue in background",
};

const zhHans: UpdateTexts = {
  promptTitle: "发现新版本",
  promptDescription: (latest, current) =>
    `新版本 ${latest} 可用，当前版本 ${current}`,
  releaseNotesLabel: "更新内容",
  laterButton: "稍后提醒",
  updateButton: "立即更新",
  downloadingButton: "下载中…",
  forceTitle: "需要更新",
  forceDescription: (latest, current) =>
    `当前版本 ${current} 已不再支持，请更新到最新版本 ${latest}`,
  progressTitle: "正在下载更新",
  settingsTitle: "软件更新",
  checkButton: "检查更新",
  checkingButton: "检查中…",
  upToDate: "已是最新版本。",
  updateAvailable: (latest) => `发现新版本 ${latest}。`,
  currentVersionLabel: (current) => `当前版本 ${current}`,
  checkFailed: "检查更新失败。",
  retryButton: "重试",

  installButton: "立即安装",
  readyHint: "更新已就绪，点击安装",
  canceledRetry: "已取消安装，可以再试一次",
  foregroundRequiredHint: "回到应用即可继续安装。",
  backgroundButton: "后台下载",
};

export const updateTextPresets: Record<UpdateLocale, UpdateTexts> = {
  en,
  "zh-Hans": zhHans,
};

function currentUpdateLocale(): UpdateLocale {
  return isSupportedLanguage(i18n.locale) ? i18n.locale : DEFAULT_LANGUAGE;
}

/** 取某 locale 的预设并叠加覆盖项。 */
export function resolveUpdateTexts(
  locale: UpdateLocale = currentUpdateLocale(),
  overrides?: Partial<UpdateTexts>,
): UpdateTexts {
  return { ...updateTextPresets[locale], ...overrides };
}
