import { createMDX } from "fumadocs-mdx/next";

const withMDX = createMDX();

// GitHub Pages 子路径前缀：仅 CI 部署时经 PAGES_BASE_PATH 注入（本地 build 留空便于验证）。
// 仓库名实际大小写为 SwarmDrop，project 站点路径区分大小写。
const basePath = process.env.PAGES_BASE_PATH ?? "";

/** @type {import('next').NextConfig} */
const config = {
  output: "export",
  reactStrictMode: true,
  basePath,
  env: { NEXT_PUBLIC_BASE_PATH: basePath },
  // 静态导出无图片优化 server
  images: { unoptimized: true },
  // 避免 GitHub Pages 目录路由 404
  trailingSlash: true,
  // 仓库根有主 app 的 pnpm-lock.yaml，Next 会误把上级当 workspace root；
  // 显式锁到 docs 目录，消除多 lockfile 警告并稳定产物追踪。
  turbopack: { root: import.meta.dirname },
};

export default withMDX(config);
