import { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { I18nProvider } from "@lingui/react";
import { i18n } from "@lingui/core";
import { ThemeProvider } from "next-themes";
import { routeTree } from "./routeTree.gen";
import { useAuthStore } from "@/stores/auth-store";
import { waitForPreferencesHydration } from "@/stores/preferences-store";
import { rehydrateSecretStore } from "@/stores/secret-store";
import { Toaster } from "@/components/ui/sonner";
import "./index.css";

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
    // 等待偏好设置 hydration 完成（主题和语言在 onRehydrateStorage 中自动应用）
    useAuthStore.getState().checkSetupStatus();
    Promise.all([
      waitForPreferencesHydration(),
      rehydrateSecretStore(),
      useAuthStore.getState().checkBiometricAvailability(),
    ]).then(() => setIsLoaded(true));
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
