/**
 * 更新 UI 的文案预设。
 *
 * ⚠️ **本文件拉自 `@swarmhive` registry,但已就地接上本仓 i18n —— 重新拉取会把这些接线抹掉**
 * (registry 版是框架无关的 `"en" | "zh-CN"` 预设,且 `resolveUpdateTexts` 缺省 `"en"`)。
 * 若需 `shadcn add @swarmhive/update-texts`,拉完请 `git diff` 复核并还原本文件。
 *
 * 与 registry 版的两处差异:
 *   1. locale 收敛到本仓的 `LocaleKey`(zh / zh-TW / en),而非 registry 的 `"en" | "zh-CN"`。
 *   2. `resolveUpdateTexts` 的缺省 locale 改为**跟随 lingui 活动语言**,而非硬编码 `"en"`。
 *      registry 版把「传 locale」的责任推给每个挂载点,而挂载点忘了传就静默变全英文
 *      —— v0.7.6 的更新弹窗就是这么整套英文的。缺省跟随活动语言才让这个 bug 不可能发生。
 */
import { i18n } from "@lingui/core";
import { defaultLocale, isLocaleKey, type LocaleKey } from "@/lib/i18n";

export type UpdateLocale = LocaleKey;

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
  /** 安装/重启中按钮。 */
  restartingButton: string;
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
}

const en: UpdateTexts = {
  promptTitle: "Update available",
  promptDescription: (latest, current) => `Version ${latest} is available (current ${current}).`,
  releaseNotesLabel: "What's new",
  laterButton: "Later",
  updateButton: "Update now",
  downloadingButton: "Downloading…",
  restartingButton: "Restarting…",
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
};

const zh: UpdateTexts = {
  promptTitle: "发现新版本",
  promptDescription: (latest, current) => `新版本 ${latest} 可用，当前版本 ${current}`,
  releaseNotesLabel: "更新内容",
  laterButton: "稍后提醒",
  updateButton: "立即更新",
  downloadingButton: "下载中…",
  restartingButton: "正在重启…",
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
};

const zhTW: UpdateTexts = {
  promptTitle: "發現新版本",
  promptDescription: (latest, current) => `新版本 ${latest} 可用，目前版本 ${current}`,
  releaseNotesLabel: "更新內容",
  laterButton: "稍後提醒",
  updateButton: "立即更新",
  downloadingButton: "下載中…",
  restartingButton: "正在重新啟動…",
  forceTitle: "需要更新",
  forceDescription: (latest, current) =>
    `目前版本 ${current} 已不再支援，請更新到最新版本 ${latest}`,
  progressTitle: "正在下載更新",
  settingsTitle: "軟體更新",
  checkButton: "檢查更新",
  checkingButton: "檢查中…",
  upToDate: "已是最新版本。",
  updateAvailable: (latest) => `發現新版本 ${latest}。`,
  currentVersionLabel: (current) => `目前版本 ${current}`,
  checkFailed: "檢查更新失敗。",
  retryButton: "重試",
};

export const updateTextPresets: Record<UpdateLocale, UpdateTexts> = {
  en,
  zh,
  "zh-TW": zhTW,
};

/**
 * 当前应展示的语言 —— 跟随 lingui 的活动语言（`i18n.locale` 类型只是 string，故需收窄）。
 * `<I18nProvider>` 在语言切换时会重渲整棵树，弹窗随之重新 resolve。
 */
function currentUpdateLocale(): UpdateLocale {
  return isLocaleKey(i18n.locale) ? i18n.locale : defaultLocale;
}

/** 取某 locale 的预设并叠加覆盖项；不传 locale 则跟随应用当前语言。 */
export function resolveUpdateTexts(
  locale: UpdateLocale = currentUpdateLocale(),
  overrides?: Partial<UpdateTexts>,
): UpdateTexts {
  return { ...updateTextPresets[locale], ...overrides };
}
