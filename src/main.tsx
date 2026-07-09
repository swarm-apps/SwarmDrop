import { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { I18nProvider } from "@lingui/react";
import { i18n } from "@lingui/core";
import { ThemeProvider } from "next-themes";
import { routeTree } from "./routeTree.gen";
import { waitForPreferencesHydration } from "@/stores/preferences-store";
import { rehydrateSecretStore } from "@/stores/secret-store";
import { syncDeviceNameFromBackend } from "@/lib/device-name";
import { Toaster } from "@/components/ui/sonner";
import "./index.css";

// e2e/desktop 下的 WebdriverIO 原生模式测试依赖 window.wdioTauri（browser.tauri.execute /
// IPC mock / 日志采集），仅 dev build 引入，避免打进生产包。
if (import.meta.env.DEV) {
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
    // 等待偏好设置 hydration 完成（主题和语言在 onRehydrateStorage 中自动应用），
    // 然后用后端持久化的设备名覆盖前端缓存（后端 = source of truth）
    Promise.all([waitForPreferencesHydration(), rehydrateSecretStore()])
      .then(() => syncDeviceNameFromBackend())
      .finally(() => setIsLoaded(true));
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
