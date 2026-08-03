import { join } from "node:path";
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
  // turbopack 的 `root` 是**文件系统边界**，不只是 lockfile 探测的起点：落在它之外的
  // 文件进不了模块图。`@swarmdrop/shared-view` 经 `link:` 指向仓库根的
  // `packages/shared-view`，所以 root 必须放到仓库根——锁在 docs/ 时，`pnpm typecheck`
  // 全绿（tsc 沿 symlink 解析得到）但 `next build` 报 `Module not found`，
  // 只有构建那一步会红。
  //
  // 这一行原先锁在 `import.meta.dirname`，为的是消除「仓库根还有一份 pnpm-lock.yaml」
  // 触发的多 lockfile 警告。显式指向仓库根同样没有该警告（歧义来自推断，不是位置），
  // 所以那个目的没有丢。
  turbopack: { root: join(import.meta.dirname, "..") },
  // 共享包发布的是 **TS 源**而非预构建产物（openspec: web-ux-alignment 的 design D2）。
  // Next 默认不转译 node_modules 下的包，必须显式登记；否则解析 `.ts` 时直接报语法错误。
  transpilePackages: ["@swarmdrop/shared-view"],
  experimental: {
    // Lingui 的宏（`<Trans>` / `` t`` ``）是编译期展开的，需要一个编译器插件。
    // 走 Next 原生的 SWC 管线，不额外挂 babel——后者会让整个 `app/` 多一层转译。
    //
    // ⚠️ SWC 插件的 ABI 与 Next 内置的 `swc_core` 版本绑定，**升 Next 时要一起验**：
    // 不匹配的表现是构建期 panic 而不是一句清晰的版本错误。
    swcPlugins: [["@lingui/swc-plugin", {}]],
  },
};

export default withMDX(config);
