export const appName = "SwarmDrop";
export const appTagline = "Drop files anywhere. No cloud. No limits.";
export const appIconPath = "/app-icon.svg";
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
