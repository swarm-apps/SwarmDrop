/**
 * CloseBehaviorManager
 *
 * 全局挂载（__root）：唯一拦截窗口 ✕（onCloseRequested），按 `closeBehavior`
 * 决定「最小化到托盘 / 退出 / 首次询问」；并消费托盘 emit 的信号（打开接收文件夹 /
 * 跳设置）。macOS `Cmd+Q` 走应用级退出、不经本拦截，天然真退出（✕ 与 Cmd+Q 语义分离）。
 */

import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openPath } from "@tauri-apps/plugin-opener";
import { useNavigate } from "@tanstack/react-router";
import { Trans, useLingui } from "@lingui/react/macro";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { commands, events } from "@/lib/bindings";
import { usePreferencesStore } from "@/stores/preferences-store";
import { isMac } from "@/lib/utils";

/** 缩盘后首次提示「仍在后台」（仅一次）。在组件内调用以便用已激活的翻译。 */
async function sendTrayHintNotification(title: string, body: string) {
  try {
    const { isPermissionGranted, requestPermission, sendNotification } =
      await import("@tauri-apps/plugin-notification");
    let granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
    if (granted) {
      sendNotification({ title, body });
    }
  } catch (err) {
    console.error("tray hint notification failed:", err);
  }
}

export function CloseBehaviorManager() {
  const { t } = useLingui();
  const navigate = useNavigate();
  const [askOpen, setAskOpen] = useState(false);
  const [remember, setRemember] = useState(false);
  const closeBehavior = usePreferencesStore((s) => s.closeBehavior);
  const savePath = usePreferencesStore((s) => s.transfer.savePath);
  const hasShownTrayHint = usePreferencesStore((s) => s.hasShownTrayHint);
  const setHasShownTrayHint = usePreferencesStore((s) => s.setHasShownTrayHint);
  const setCloseBehavior = usePreferencesStore((s) => s.setCloseBehavior);

  const trayWord = isMac ? t`菜单栏` : t`托盘`;

  const hideToTray = useCallback(async () => {
    await getCurrentWindow().hide();
    if (!hasShownTrayHint) {
      await sendTrayHintNotification(
        t`SwarmDrop 仍在后台运行`,
        t`应用已最小化到${trayWord}，仍可接收文件。退出请在${trayWord}图标的菜单中选择「退出」。`,
      );
      setHasShownTrayHint(true);
    }
  }, [hasShownTrayHint, setHasShownTrayHint, t, trayWord]);

  const quit = useCallback(async () => {
    await commands.quitApp();
  }, []);

  // 唯一拦截 ✕：始终 preventDefault，按 closeBehavior 显式执行。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void getCurrentWindow()
      .onCloseRequested(async (event) => {
        event.preventDefault();
        if (closeBehavior === "tray") {
          await hideToTray();
        } else if (closeBehavior === "quit") {
          await quit();
        } else {
          setRemember(false); // 每次询问都从未勾选开始，避免上次残留导致误持久化
          setAskOpen(true);
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [closeBehavior, hideToTray, quit]);

  // 托盘信号（类型化 tauri-specta 事件）：打开接收文件夹 / 跳设置（路径与路由由前端拥有）。
  useEffect(() => {
    let disposed = false;
    const unlistens: Array<() => void> = [];
    const track = (u: () => void) => {
      if (disposed) u();
      else unlistens.push(u);
    };
    void events.trayOpenReceiveFolder
      .listen(async () => {
        if (savePath) {
          try {
            await openPath(savePath);
          } catch (err) {
            console.error("open receive folder failed:", err);
          }
        }
      })
      .then(track);
    void events.trayOpenSettings
      .listen(() => {
        void navigate({ to: "/settings" });
      })
      .then(track);
    return () => {
      disposed = true;
      unlistens.forEach((u) => u());
    };
  }, [navigate, savePath]);

  const onMinimize = useCallback(async () => {
    if (remember) setCloseBehavior("tray");
    setAskOpen(false);
    await hideToTray();
  }, [remember, hideToTray, setCloseBehavior]);

  const onQuit = useCallback(async () => {
    if (remember) setCloseBehavior("quit");
    setAskOpen(false);
    await quit();
  }, [remember, quit, setCloseBehavior]);

  return (
    <AlertDialog open={askOpen} onOpenChange={setAskOpen}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            <Trans>关闭 SwarmDrop 窗口？</Trans>
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t`SwarmDrop 需要在后台保持在线，才能接收已配对设备发来的文件。可以让它继续在${trayWord}后台运行，或彻底退出。退出后将无法被发现，也不会收到任何文件。`}
          </AlertDialogDescription>
        </AlertDialogHeader>

        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            type="checkbox"
            className="size-4 accent-primary"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          <Trans>记住我的选择（之后可在「设置」中修改）</Trans>
        </label>

        <AlertDialogFooter>
          <Button variant="outline" onClick={onQuit}>
            <Trans>退出 SwarmDrop</Trans>
          </Button>
          <Button onClick={onMinimize}>{t`最小化到${trayWord}`}</Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
