import { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { I18nProvider } from "@lingui/react";
import { i18n } from "@lingui/core";
import { ThemeProvider } from "next-themes";
import { routeTree } from "./routeTree.gen";
import { waitForPreferencesHydration } from "@/stores/preferences-store";
import { rehydrateSecretStore } from "@/stores/secret-store";
import { autoStartNodeIfEnabled } from "@/stores/network-store";
import { syncDeviceNameFromBackend } from "@/lib/device-name";
import { Toaster } from "@/components/ui/sonner";
import "./index.css";

// e2e/desktop 下的 WebdriverIO 原生模式测试依赖 window.wdioTauri（browser.tauri.execute /
// IPC mock / 日志采集）。Vite dev 天然引入；录制用 debug no-bundle 会走生产构建，
// 由录制脚本显式传 VITE_WDIO_TAURI_PLUGIN=1。正常 release 不打进生产包。
if (import.meta.env.DEV || import.meta.env.VITE_WDIO_TAURI_PLUGIN === "1") {
  void import("@wdio/tauri-plugin");
}

// Create a new router instance
const router = createRouter({ routeTree });

// Register the router instance for type safety
declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

function App() {
  const [isLoaded, setIsLoaded] = useState(false);

  useEffect(() => {
    // ── 冷启动序列 ──
    // 四步，其中一条**承重的**顺序约束（见下面 autoStart 处）。它现在还摊在这里，
    // 而另两端都有具名落点（Web 是 `_lib/node-lifecycle.ts`，移动是 `mobile-core-store.ts`）。
    // **抽成 `src/lib/app-bootstrap.ts` 的触发条件：出现第 5 步，或出现第二条顺序约束。**
    // 理由：本文件是前端唯一不可能被单测覆盖的地方（同时做 router 注册 / theme / i18n /
    // wdio 注入），顺序约束住在这里，强度就等于「下一个人会读这段注释」。
    //
    // 等待偏好设置 hydration 完成（主题和语言在 onRehydrateStorage 中自动应用），
    // 然后用后端持久化的设备名覆盖前端缓存（后端 = source of truth）
    Promise.all([waitForPreferencesHydration(), rehydrateSecretStore()])
      .then(() => syncDeviceNameFromBackend())
      .finally(() => {
        setIsLoaded(true);
        // 冷启动自动启动挂在这里，且**必须排在 syncDeviceNameFromBackend 之后**：
        // 节点启动时 identify 广播的 agent_version 取自那次同步的结果，顺序颠倒会让
        // 冷启动那一次对端看到旧设备名（移动端 mobile-core-store.ts 踩过并留了同一条注释）。
        // 不 await：节点启动含网络绑定与 bootstrap 拨号，进首屏门禁会让开了自动启动的
        // 用户每次多盯几秒白屏。
        void autoStartNodeIfEnabled();
      });
  }, []);

  if (!isLoaded) {
    return null;
  }

  return (
    <I18nProvider i18n={i18n}>
      <ThemeProvider
        attribute="class"
        defaultTheme="system"
        enableSystem
        disableTransitionOnChange
        storageKey="theme"
      >
        <RouterProvider router={router} />
        <Toaster />
      </ThemeProvider>
    </I18nProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <App />,
);
