import path from "node:path";
import { lingui } from "@lingui/vite-plugin";
import react from "@vitejs/plugin-react";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    react({
      babel: {
        plugins: ["@lingui/babel-plugin-lingui-macro"],
      },
    }),
    lingui(),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    // 三个独立子项目都不归这份配置管，显式排除避免 glob 误扫：
    //   e2e/desktop  WebdriverIO 原生模式 E2E（自己的 package.json / mocha framework）
    //   mobile/      RN 的 pnpm workspace（自己的 package.json / RN 测试环境）
    //   docs/        Next 的 pnpm workspace，依赖装在 docs/node_modules，
    //                有自己的 docs/vitest.config.ts
    // exclude 会整体覆盖默认值，必须展开 configDefaults.exclude 而不是只写新增项。
    exclude: [...configDefaults.exclude, "e2e/**", "mobile/**", "docs/**"],
  },
});
