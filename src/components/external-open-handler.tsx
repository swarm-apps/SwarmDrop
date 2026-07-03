/**
 * ExternalOpenHandler
 *
 * 入站「用 SwarmDrop 打开」处理器（空渲染、命令式），挂在 `_app` 布局。对标
 * SwarmDrop-RN 根布局的 ShareIntentHandler。流程：
 *   1. 挂载时先订阅 `external-file-open` 事件，再拉取冷启动期间缓冲的路径
 *      （顺序很重要：先订阅后拉取，避免 take 标记就绪后、订阅前到达的路径丢失）。
 *   2. 未设置设备名（仍在首启引导）→ toast 提示并丢弃本次（与移动端 v1 一致，不缓冲回放）。
 *   3. 已设置 → 包装成 FileSource[] 塞进 share-store → 跳 `/send/share-target`。
 */

import { useEffect } from "react";
import { useNavigate } from "@tanstack/react-router";
import { t } from "@lingui/core/macro";
import { toast } from "sonner";
import { commands, events, type FileSource } from "@/lib/bindings";
import { usePreferencesStore } from "@/stores/preferences-store";
import { useShareStore } from "@/stores/share-store";

export function ExternalOpenHandler() {
  const navigate = useNavigate();

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    const handle = (paths: string[]) => {
      if (paths.length === 0) return;

      // 首启未设设备名 → 丢弃本次意图并提示（不缓冲回放）。
      const deviceName = usePreferencesStore.getState().deviceName.trim();
      if (deviceName === "") {
        toast.info(t`请先完成 SwarmDrop 设置`);
        return;
      }

      const sources: FileSource[] = paths.map((path) => ({ type: "path", path }));
      useShareStore.getState().setSources(sources);
      void navigate({ to: "/send/share-target" });
    };

    void (async () => {
      unlisten = await events.externalFileOpen.listen((event) => {
        handle(event.payload.paths);
      });
      if (cancelled) {
        unlisten();
        return;
      }
      // 拉取冷启动竞态期间缓冲的路径（取走即清空）。
      const pending = await commands.takePendingExternalOpen();
      if (!cancelled) handle(pending);
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [navigate]);

  return null;
}
