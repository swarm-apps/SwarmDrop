export type UpdateLocale = "en" | "zh-CN";

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

  // —— RN 专属(native APK 安装层)——
  // Tauri 端用不到这些键;它们在 RN 的「下载完成 → 系统安装器接管」语义里出现。
  /** ready 态主按钮:点击拉起系统安装器。 */
  installButton: string;
  /** install() 已 handoff 给系统、等待系统确认弹窗时的提示(ready 态)。 */
  systemConfirmHint: string;
  /** 未授权"安装未知应用"时的引导文案(install 入口门禁)。 */
  unknownSourceHint: string;
  /** 用户在系统确认框点了取消后的温和提示(非红条错误)。 */
  canceledRetry: string;
}

const en: UpdateTexts = {
  promptTitle: "Update available",
  promptDescription: (latest, current) =>
    `Version ${latest} is available (current ${current}).`,
  releaseNotesLabel: "What's new",
  laterButton: "Later",
  updateButton: "Update now",
  downloadingButton: "Downloading…",
  restartingButton: "Installing…",
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
  systemConfirmHint: "Waiting for the system installer…",
  unknownSourceHint:
    "Allow installing from this app in Settings, then return to continue.",
  canceledRetry: "Installation canceled. You can try again.",
};

const zhCN: UpdateTexts = {
  promptTitle: "发现新版本",
  promptDescription: (latest, current) =>
    `新版本 ${latest} 可用，当前版本 ${current}`,
  releaseNotesLabel: "更新内容",
  laterButton: "稍后提醒",
  updateButton: "立即更新",
  downloadingButton: "下载中…",
  restartingButton: "正在安装…",
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

  installButton: "点击安装",
  systemConfirmHint: "系统弹窗确认中…",
  unknownSourceHint: "请在系统设置里允许本应用安装未知应用，返回后继续。",
  canceledRetry: "已取消，可重试",
};

export const updateTextPresets: Record<UpdateLocale, UpdateTexts> = {
  en,
  "zh-CN": zhCN,
};

/** 取某 locale 的预设并叠加覆盖项。 */
export function resolveUpdateTexts(
  locale: UpdateLocale = "en",
  overrides?: Partial<UpdateTexts>,
): UpdateTexts {
  return { ...updateTextPresets[locale], ...overrides };
}
