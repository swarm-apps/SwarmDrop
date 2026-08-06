import { defineConfig } from "@lingui/cli";
import { formatter } from "@lingui/format-po";

export default defineConfig({
  sourceLocale: "zh",
  locales: ["zh", "zh-TW", "en"],
  catalogs: [
    {
      path: "<rootDir>/src/locales/{locale}/messages",
      // `packages/file-browser` 是桌面与 Web 共用的组件包，组件内联 Lingui 宏。
      // 两端各自 extract 到自己的 catalog（三端 catalog 独立是既定约定，见 CLAUDE.md），
      // 所以同一句话在桌面与 Web 的 .po 里各存一份——这是约定的必然结果，不是重复劳动。
      // 注意路径没有 `../`：本文件就在仓库根，`rootDir` 即仓库根。
      // Web 那份配置住在 `docs/`，所以那边写的是 `../packages/file-browser/src`。
      include: ["src", "packages/file-browser/src"],
    },
  ],
  format: formatter({ lineNumbers: false }),
});
