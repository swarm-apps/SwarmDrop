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
    // e2e/desktop 是独立的 WebdriverIO 原生模式 E2E 项目（自己的 package.json / mocha
    // framework），不归 Vitest 管，显式排除避免 glob 误扫。exclude 会整体覆盖默认值，
    // 必须展开 configDefaults.exclude 而不是只写新增项。
    exclude: [...configDefaults.exclude, "e2e/**"],
  },
});
