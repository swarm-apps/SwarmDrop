import { defineConfig } from "@lingui/cli";

export default defineConfig({
  sourceLocale: "zh-Hans",
  locales: ["zh-Hans", "en"],
  catalogs: [
    {
      path: "<rootDir>/src/locales/{locale}/messages",
      include: ["src"],
      exclude: ["src/locales/**"],
    },
  ],
});
