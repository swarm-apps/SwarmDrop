import type { MetadataRoute } from "next";
import { SITE_URL } from "@/lib/site";
import { source } from "@/lib/source";

// 静态导出的 sitemap.xml。trailingSlash 开启，URL 统一带尾斜杠；绝对 URL 含子路径。
export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  const home = { url: `${SITE_URL}/`, priority: 1 };
  const docs = source.getPages().map((page) => ({
    url: `${SITE_URL}${page.url}/`,
    priority: 0.7,
  }));
  return [home, ...docs];
}
