import { defineConfig } from "@lingui/cli";
import { formatter } from "@lingui/format-po";

/**
 * Web 应用区的翻译目录。
 *
 * **范围只有 `app/app`**：文档正文（`content/docs`）与营销页不走 Lingui，它们的多语言是
 * 另一件事（fumadocs 有自己的 i18n 路由方案），混进来会把整站的 msgid 全扫进同一份目录。
 *
 * locale 集合与桌面 (`../lingui.config.ts`) 保持一致，但**目录不共用**——两端的文案没有重叠，
 * 合并只会让任一端的提取扫到对方的 msgid。
 */
export default defineConfig({
  sourceLocale: "zh",
  locales: ["zh", "zh-TW", "en"],
  catalogs: [
    {
      path: "<rootDir>/app/app/_locales/{locale}/messages",
      // `packages/file-browser` 同理并入（说明见桌面 `../lingui.config.ts` 的同一处）。
      include: ["app/app", "../packages/file-browser/src"],
    },
  ],
  format: formatter({ lineNumbers: false }),
});
