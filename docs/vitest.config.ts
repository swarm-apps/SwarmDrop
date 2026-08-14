import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

/**
 * docs 是独立的 pnpm workspace，依赖装在 `docs/node_modules`——所以它需要自己的 vitest，
 * 不能靠仓库根那份跑。仓库根的 `vitest.config.ts` 已显式排除 `docs/**`，两者不重叠。
 *
 * 目前只覆盖纯逻辑（`lib/` 与 `app/app/_lib/`）。要测 React 组件时再加 jsdom 环境与
 * `@testing-library/react`——那时也要把 Lingui 的 SWC 宏**真正**接进来（见下方 alias 的
 * 说明：现在给的是替身，够纯逻辑用，测不了「渲染出的译文」）。
 */
export default defineConfig({
  resolve: {
    alias: [
      // `_lib/*.ts` 用 `@/lib/cn` 这类路径别名（tsconfig paths），vitest 不读 tsconfig。
      {
        find: /^@\//,
        replacement: fileURLToPath(new URL("./", import.meta.url)),
      },
      // Lingui 的编译期宏在 vitest 里没有展开器——替身见该文件的说明。
      {
        find: /^@lingui\/core\/macro$/,
        replacement: fileURLToPath(
          new URL("./test/lingui-macro-stub.ts", import.meta.url),
        ),
      },
      {
        find: /^@lingui\/react\/macro$/,
        replacement: fileURLToPath(
          new URL("./test/lingui-react-macro-stub.tsx", import.meta.url),
        ),
      },
    ],
  },
  test: {
    include: ["lib/**/*.test.ts", "app/**/*.test.{ts,tsx}"],
  },
});
