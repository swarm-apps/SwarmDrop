import { BASE_PATH } from "./site";

export const appName = "SwarmDrop";
export const appTagline = "Drop files anywhere. No cloud. No limits.";
// GitHub Pages 部署在 /SwarmDrop 子路径下，纯字符串路径（不像 next/image、next/link
// 那样能被框架自动加前缀）必须手动拼 BASE_PATH，否则会被解析成域名根路径 404。
export const appIconPath = `${BASE_PATH}/app-icon.svg`;
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
  channel: "stable",
};

/** 各平台下载与相关链接。 */
export const links = {
  downloads: "#download",
  releases: `https://github.com/${gitConfig.user}/${gitConfig.repo}/releases/latest`,
  repo: `https://github.com/${gitConfig.user}/${gitConfig.repo}`,
  mobile: "https://github.com/swarm-apps/SwarmDrop-RN/releases",
};
