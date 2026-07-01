import type { Metadata } from "next";
import { Provider } from "@/components/provider";
import { appIconPath, appName, appTagline } from "@/lib/shared";
import { BASE_PATH, SITE_ORIGIN } from "@/lib/site";
import "./global.css";

export const metadata: Metadata = {
  // 绝对 URL 基准：让相对 OG/twitter 图正确解析
  metadataBase: new URL(SITE_ORIGIN),
  title: { default: `${appName} · ${appTagline}`, template: `%s · ${appName}` },
  description:
    "SwarmDrop 是去中心化、跨网络、端到端加密的 P2P 文件传输工具。无账号、无服务器，把 LocalSend 的体验扩展到任意网络。",
  icons: {
    icon: [
      { url: appIconPath, type: "image/svg+xml" },
      { url: `${BASE_PATH}/favicon.ico`, sizes: "any" },
    ],
  },
};

export default function Layout({ children }: LayoutProps<"/">) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <body className="flex flex-col min-h-screen antialiased">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
