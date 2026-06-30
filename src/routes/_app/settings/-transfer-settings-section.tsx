/**
 * TransferSettingsSection
 * 设置页「文件传输」区域 — 传输相关设置
 */

import { useCallback, useEffect, useState } from "react";
import { Trans } from "@lingui/react/macro";
import { useLingui } from "@lingui/react/macro";
import { FolderOpen, HardDrive, Pencil } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { usePreferencesStore } from "@/stores/preferences-store";
import { homeDir } from "@tauri-apps/api/path";
import { pickFolder, getDefaultSavePath } from "@/lib/file-picker";
import { toast } from "sonner";
import { SettingsCard, SettingsSection } from "./-settings-primitives";

export function TransferSettingsSection() {
  const { t } = useLingui();
  const { savePath, setTransferSavePath } = usePreferencesStore(
    useShallow((state) => ({
      savePath: state.transfer.savePath,
      setTransferSavePath: state.setTransferSavePath,
    })),
  );

  const [displayPath, setDisplayPath] = useState(() => t`<未设置>`);

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
      toast.error(t`无法打开文件夹选择器，请检查存储权限`);
    }
  }, [savePath, setTransferSavePath, t]);

  return (
    <SettingsSection title={<Trans>文件传输</Trans>} icon={HardDrive}>
      <SettingsCard>
        <button
          type="button"
          onClick={handleChangePath}
          className="group flex w-full flex-col gap-3 p-4 text-left transition-colors hover:bg-accent/40"
        >
          <div className="flex w-full items-start justify-between gap-3">
            <div className="flex min-w-0 items-start gap-3">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-xl bg-blue-500/10 text-blue-600 dark:text-blue-400">
                <FolderOpen className="size-5" />
              </span>
              <div className="min-w-0">
                <span className="text-sm font-medium text-foreground">
                  <Trans>保存位置</Trans>
                </span>
                <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
                  <Trans>接收文件会默认保存到这里，可随时更改。</Trans>
                </span>
              </div>
            </div>
            <span className="flex shrink-0 items-center gap-1.5 rounded-full border border-border bg-background/70 px-2.5 py-1 text-[11px] font-medium text-foreground transition-colors group-hover:border-blue-500/30 group-hover:text-blue-600 dark:bg-white/[0.04] dark:group-hover:text-blue-400">
              <Pencil className="size-3" />
              <Trans>更改</Trans>
            </span>
          </div>

          <div className="flex w-full min-w-0 items-center gap-2 rounded-xl border border-border/70 bg-background/55 px-3 py-2.5 dark:bg-white/[0.035]">
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
