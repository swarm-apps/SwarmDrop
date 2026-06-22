// 站点 URL 常量。NEXT_PUBLIC_BASE_PATH 由 next.config 注入（CI 部署 = "/SwarmDrop"，本地 = ""）。
// 用来判定生产 origin（供 metadataBase / sitemap 出绝对 URL），并在客户端拼绝对路径。

export const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? "";

/** 生产 origin（GitHub Pages）；本地 build 用 localhost，避免 OG/sitemap 出 localhost 绝对 URL。 */
export const SITE_ORIGIN = BASE_PATH ? "https://swarm-apps.github.io" : "http://localhost:3000";

/** 站点根（含子路径）。 */
export const SITE_URL = `${SITE_ORIGIN}${BASE_PATH}`;
