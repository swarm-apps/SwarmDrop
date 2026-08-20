/**
 * ExternalOpenHandler
 *
 * 入站外部入口处理器（空渲染、命令式），挂在 `_app` 布局。两类负载同一条通路
 * （openspec: pair-deep-link design D2）：
 *
 * | 负载 | 来源 | 去处 |
 * |---|---|---|
 * | 文件路径 | 「用 SwarmDrop 打开」 | share-store → `/send/share-target` |
 * | 配对邀请 | `swarmdrop://` 深链 | previewInvite → 确认卡 → `/pairing/input` |
 *
 * 流程：
 *   1. 挂载时**先订阅两个事件，再拉取**冷启动期间缓冲的负载
 *      （顺序很重要：先订阅后拉取，避免 take 标记就绪后、订阅前到达的负载丢失）。
 *   2. 未设置设备名（仍在首启引导）→ toast 提示并丢弃本次（不缓冲回放）。
 *   3. 文件 → 包装成 FileSource[] 塞进 share-store → 跳选设备屏；
 *      邀请 → 走 pairing-store 的 previewInvite（解码验签 → 确认卡），**不自动配对**。
 *
 * 两类负载由**一次** `takePendingExternalOpen()` 取走：后端的 `frontend_ready` 是共享标记，
 * 拆成两次调用会让第二类负载丢在「标记已置位、订阅还没完成」的缝里。
 */

import { useEffect } from "react";
import { useNavigate } from "@tanstack/react-router";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import { commands, events } from "@/lib/bindings";
import { pathsToSources } from "@/lib/file-source";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useShareStore } from "@/stores/share-store";
import { usePairingStore } from "@/stores/pairing-store";

export function ExternalOpenHandler() {
  const navigate = useNavigate();
  const deviceName = usePreferencesStore((s) => s.deviceName);
  const setShareSources = useShareStore((s) => s.setSources);
  const previewInvite = usePairingStore((s) => s.previewInvite);

  useEffect(() => {
    let unlistenFiles: (() => void) | undefined;
    let unlistenInvite: (() => void) | undefined;
    let cancelled = false;

    /** 首启未设设备名时的共同闸：丢弃本次意图并提示（不缓冲回放）。 */
    const readyForIntent = () => {
      if (deviceName.trim() === "") {
        toast.info(t`请先完成 SwarmDrop 设置`);
        return false;
      }
      return true;
    };

    const handleFiles = (paths: string[]) => {
      if (paths.length === 0 || !readyForIntent()) return;
      setShareSources(pathsToSources(paths));
      void navigate({ to: "/send/share-target" });
    };

    const handleInvite = (invite: string) => {
      if (invite.trim() === "" || !readyForIntent()) return;
      // 先跳受邀方屏再 preview：解码失败时 pairing-store 会 toast 报错，用户停在那一屏
      // 正好可以改用粘贴，而不是被留在一个和配对无关的页面上看一句错误。
      void navigate({ to: "/pairing/input" });
      void previewInvite(invite);
    };

    void (async () => {
      unlistenFiles = await events.externalFileOpen.listen((event) => {
        handleFiles(event.payload.paths);
      });
      unlistenInvite = await events.externalPairInvite.listen((event) => {
        handleInvite(event.payload.invite);
      });
      if (cancelled) {
        unlistenFiles();
        unlistenInvite();
        return;
      }
      // 拉取冷启动竞态期间缓冲的负载（取走即清空，两类一次拿完）。
      const pending = await commands.takePendingExternalOpen();
      if (cancelled) return;
      handleFiles(pending.paths);
      if (pending.invite !== null) handleInvite(pending.invite);
    })();

    return () => {
      cancelled = true;
      unlistenFiles?.();
      unlistenInvite?.();
    };
  }, [deviceName, navigate, setShareSources, previewInvite]);

  return null;
}
