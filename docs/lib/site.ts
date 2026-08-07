// 站点 URL 常量。两个 PAGES_* 都由 CI 提供（见 .github/workflows/docs.yml），
// 本地 build 都留空 → localhost 根路径，便于直接验证。
//
// **站点部署在 Pages 子路径下**，所以裸字符串路径（`<img src>`、metadata 里的 `/x`）
// 必须手动拼 BASE_PATH——`next/link` / `next/image` 才会自动加前缀。理由见 next.config.mjs。

/** 生产 origin；无尾斜杠。只有构建期服务端读它（metadataBase / sitemap）。 */
export const SITE_ORIGIN =
  process.env.PAGES_SITE_ORIGIN || "http://localhost:3000";

/** 子路径前缀（CI = "/SwarmDrop"，本地 = ""）；与 next.config.mjs 的 basePath 同源。 */
export const BASE_PATH = process.env.PAGES_BASE_PATH ?? "";

// CI 下缺这个值必须**炸**，不能静默回落。
//
// 迁移前它失效是「响的」：basePath 一空，_next/* 全断，一眼可见。现在的失效是安静的
// —— `next build` 成功，产出 `metadataBase: http://localhost:3000` 与一整份 localhost
// sitemap，线上零可见症状，等 SEO 出问题才发现。
if (process.env.CI && !process.env.PAGES_SITE_ORIGIN) {
  throw new Error(
    "CI 构建缺少 PAGES_SITE_ORIGIN —— metadataBase 与 sitemap 会产出 localhost 绝对 URL。" +
      "请检查 .github/workflows/docs.yml 的 env。",
  );
}

/** 站点根（含子路径）。metadataBase 与 sitemap 用它。 */
export const SITE_URL = `${SITE_ORIGIN}${BASE_PATH}`;
