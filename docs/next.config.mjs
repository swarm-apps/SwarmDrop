import { createMDX } from "fumadocs-mdx/next";

const withMDX = createMDX();

// **站点仍部署在 GitHub Pages 子路径 `/SwarmDrop` 下。**
// `swarmapp.cn` 已实名但尚未备案，境内注册商可以随时停掉未备案 `.cn` 的解析，所以自定义
// 域名这一步整体延后（openspec: invite-url-canonical）。备案下来后：这里连 `basePath` 一起
// 去掉、`docs.yml` 的两个 env 改掉、`crates/invite` 的主前缀换掉，三处一起动。
//
// 子路径带来两个坑，改这里前先读明白：
// 1. **裸字符串路径要手动拼 `BASE_PATH`** —— `next/link`、`next/image` 会自动加前缀，
//    但 `<img src="/x">`、metadata 里的 `/x` 不会（`/x` 解析时会把 base path 整段替换掉）。
//    已知需要拼的：`docs/lib/shared.ts` 的 `appIconPath`。
// 2. **路径大小写必须精确匹配仓库名** `SwarmDrop`（Pages 区分大小写）。
//
// 生产 origin 与子路径都由 CI 的 env 提供，见 `.github/workflows/docs.yml`。
// CNAME 文件在本仓的 workflow 型 Pages 部署下**是无效的**，权威源是 Settings → Pages。

// 子路径前缀：仅 CI 部署时经 `PAGES_BASE_PATH` 注入（本地 build 留空便于直接验证）。
const basePath = process.env.PAGES_BASE_PATH ?? "";

/** @type {import('next').NextConfig} */
const config = {
  output: "export",
  basePath,
  reactStrictMode: true,
  // 静态导出无图片优化 server
  images: { unoptimized: true },
  // 避免 GitHub Pages 目录路由 404
  trailingSlash: true,
  // 仓库根有主 app 的 pnpm-lock.yaml，Next 会误把上级当 workspace root；
  // 显式锁到 docs 目录，消除多 lockfile 警告并稳定产物追踪。
  turbopack: { root: import.meta.dirname },
};

export default withMDX(config);
