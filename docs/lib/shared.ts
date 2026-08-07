import { BASE_PATH } from "./site";

export const appName = "SwarmDrop";
export const appTagline = "Drop files anywhere. No cloud. No limits.";
/**
 * `public/` 下静态资源的引用路径。**所有 `<Image src>` / `<img src>` 都要经过它。**
 *
 * 站点在 Pages 子路径下，裸字符串路径会被解析成域名根路径 → 404。`next/link` 会自动
 * 加前缀，但 `next/image` 在 `images.unoptimized`（静态导出必需）下**不会** —— 详见
 * `lib/site.ts`。本地 BASE_PATH 为空，写错了也看不出来，只有 Pages 上才炸。
 */
export const assetPath = (path: string) =>
  `${BASE_PATH}/${path.replace(/^\//, "")}`;

export const appIconPath = assetPath("app-icon.png");
export const docsRoute = "/docs";
export const docsImageRoute = "/og/docs";
export const docsContentRoute = "/llms.mdx/docs";

export const gitConfig = {
  user: "swarm-apps",
  repo: "SwarmDrop",
  branch: "main",
};

export const swarmhiveConfig = {
  baseUrl: process.env.NEXT_PUBLIC_SWARMHIVE_URL ?? "http://47.115.172.218:3030",
  appSlug: "swarmdrop",
  /** 移动端(Android)是独立版本线,在 SwarmHive 里是单独的 app。 */
  appSlugMobile: "swarmdrop-rn",
  channel: "stable",
};

/** 各平台下载与相关链接。 */
export const links = {
  downloads: "#download",
  releases: `https://github.com/${gitConfig.user}/${gitConfig.repo}/releases/latest`,
  repo: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  // SwarmHive 不可用时的后备下载入口。移动端已并入主仓，发版走 mobile-v* tag，
  // 所以指向主仓 releases（与桌面的 v* 混在一列，但至少能拿到最新 APK）——
  // 原先指向的 swarm-apps/SwarmDrop-RN 已归档，其 releases 永远停在 v0.7.18。
  mobile: `https://github.com/${gitConfig.user}/${gitConfig.repo}/releases`,
};
