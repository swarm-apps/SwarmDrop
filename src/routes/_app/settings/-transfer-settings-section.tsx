/**
 * TransferSettingsSection
 * 设置页「文件传输」区域 — 传输相关设置
 */

import { useCallback, useEffect, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { FolderOpen, HardDrive, Pencil } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { usePreferencesStore } from "@/stores/preferences-store";
import { homeDir } from "@tauri-apps/api/path";
import { pickFolder, getDefaultSavePath } from "@/lib/file-picker";
import { toast } from "sonner";
import { SettingsCard, SettingsSection } from "./-settings-primitives";

export function TransferSettingsSection() {
  const { savePath, setTransferSavePath } = usePreferencesStore(
    useShallow((state) => ({
      savePath: state.transfer.savePath,
      setTransferSavePath: state.setTransferSavePath,
    })),
  );

  const [displayPath, setDisplayPath] = useState("<未设置>");

  // 初始化默认保存路径
  useEffect(() => {
    if (!savePath) {
      getDefaultSavePath().then(setTransferSavePath);
    }
  }, [savePath, setTransferSavePath]);

  // 更新显示路径（使用 homeDir API 简化路径显示）
  useEffect(() => {
    if (savePath) {
      homeDir().then((home) => {
        if (home && savePath.startsWith(home)) {
          // 将用户目录前缀替换为 ~
          const relative = savePath.slice(home.length).replace(/\\/g, "/");
          setDisplayPath(`~${relative}`);
        } else {
          setDisplayPath(savePath);
        }
      });
    }
  }, [savePath]);

  const handleChangePath = useCallback(async () => {
    try {
      const selected = await pickFolder(savePath);
      if (selected) {
        setTransferSavePath(selected);
      }
    } catch (err) {
      console.error("Failed to pick folder:", err);
      toast.error("无法打开文件夹选择器，请检查存储权限");
    }
  }, [savePath, setTransferSavePath]);

  return (
    <SettingsSection title={<Trans>文件传输</Trans>} icon={HardDrive}>
      <SettingsCard>
        <button
          type="button"
          onClick={handleChangePath}
          className="group flex w-full flex-col gap-3.5 p-4 text-left hover:bg-accent/50"
        >
          <div className="grid w-full min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-3">
            <span className="flex size-11 shrink-0 items-center justify-center rounded-[17px] bg-blue-50 text-blue-600 shadow-[inset_0_1px_0_rgba(255,255,255,0.55)] dark:bg-blue-500/10 dark:text-blue-200">
              <FolderOpen className="size-5" />
            </span>
            <div className="min-w-0">
              <span className="text-sm font-semibold text-foreground">
                <Trans>保存位置</Trans>
              </span>
              <span className="mt-1 block max-w-[34ch] text-xs leading-5 text-muted-foreground">
                <Trans>接收文件会默认保存到这里，可随时更改。</Trans>
              </span>
            </div>
            <span className="flex shrink-0 items-center gap-1.5 rounded-full bg-foreground px-2.5 py-1 text-[11px] font-medium text-background transition-transform duration-300 ease-[cubic-bezier(0.32,0.72,0,1)] group-hover:-translate-y-0.5">
              <Pencil className="size-3" />
              <Trans>更改</Trans>
            </span>
          </div>

          <div className="flex w-full min-w-0 items-center gap-2.5 rounded-[18px] border border-border/70 bg-background/55 px-3.5 py-3 shadow-[inset_0_1px_0_rgba(255,255,255,0.38)] dark:bg-white/[0.035]">
            <span className="size-1.5 shrink-0 rounded-full bg-blue-500/70" />
            <span className="min-w-0 truncate font-mono text-xs text-foreground">
              {displayPath}
            </span>
          </div>
        </button>
      </SettingsCard>
    </SettingsSection>
  );
}
