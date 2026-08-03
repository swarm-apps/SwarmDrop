import { defineConfig } from "vitest/config";

/**
 * docs 是独立的 pnpm workspace，依赖装在 `docs/node_modules`——所以它需要自己的 vitest，
 * 不能靠仓库根那份跑。仓库根的 `vitest.config.ts` 已显式排除 `docs/**`，两者不重叠。
 *
 * 目前只覆盖纯逻辑（`lib/` 与 `app/app/_lib/`）。要测 React 组件时再加 jsdom 环境与
 * `@testing-library/react`——那时也要把 Lingui 的 SWC 宏接进来（vitest 走 esbuild，
 * 不吃 `next.config.mjs` 里的 swcPlugins），届时参照仓库根的 `vitest.config.ts` 用
 * babel 插件的写法。
 */
export default defineConfig({
  test: {
    include: ["lib/**/*.test.ts", "app/**/*.test.ts"],
  },
});
