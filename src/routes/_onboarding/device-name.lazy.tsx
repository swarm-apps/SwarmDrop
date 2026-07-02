/**
 * Onboarding · Device Name
 * 首启第一步：让用户为这台设备起个名字。
 *
 * 默认值取自 `tauri-plugin-os.hostname()`（macOS 上是机器名，Windows 上是
 * COMPUTERNAME）；用户可接受默认或改名。完成后调 `applyDeviceName` 写入后端
 * + 同步前端缓存，然后跳 `/devices`。
 */

import { Trans, useLingui } from "@lingui/react/macro";
import { createLazyFileRoute, useNavigate } from "@tanstack/react-router";
import { ArrowRight, MonitorSmartphone, ShieldCheck } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { hostname as readHostname } from "@tauri-apps/plugin-os";
import { Input } from "@/components/ui/input";
import { applyDeviceName } from "@/lib/device-name";
import { getErrorMessage } from "@/lib/errors";
import { usePreferencesStore } from "@/stores/preferences-store";
import {
  GlassPanel,
  InfoTile,
  TaskButton,
} from "@/components/layout/task-surface";

export const Route = createLazyFileRoute("/_onboarding/device-name")({
  component: DeviceNameStep,
});

function DeviceNameStep() {
  const { t } = useLingui();
  const navigate = useNavigate();
  const existing = usePreferencesStore((s) => s.deviceName);

  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 默认值：旧 deviceName（如果有）→ 否则系统 hostname → 否则空白
  useEffect(() => {
    if (existing.trim().length > 0) {
      setName(existing);
      return;
    }
    readHostname()
      .then((host) => setName(host ?? ""))
      .catch(() => setName(""));
  }, [existing]);

  const trimmed = name.trim();
  const disabled = saving || trimmed.length === 0;

  const onConfirm = async () => {
    setSaving(true);
    setError(null);
    try {
      await applyDeviceName(trimmed);
      toast.success(t`设备名称已设置`);
      navigate({ to: "/devices" });
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex h-full items-center justify-center p-5">
      <GlassPanel className="w-full max-w-[720px]">
        <div className="grid gap-6 p-6 md:grid-cols-[260px_minmax(0,1fr)] md:p-7">
          <div className="flex flex-col justify-between gap-6">
            <div className="flex flex-col gap-4">
              <div className="glass-accent flex size-18 items-center justify-center rounded-[28px] text-brand">
                <MonitorSmartphone className="size-8" />
              </div>
              <div>
                <p className="text-[12px] font-medium text-brand">
                  SwarmDrop
                </p>
                <h1 className="mt-1 text-2xl font-semibold tracking-tight text-foreground">
                  <Trans>给设备取个名字</Trans>
                </h1>
                <p className="mt-2 text-sm leading-6 text-muted-foreground">
                  <Trans>其他设备配对时会看到这个名称，你可以随时在设置中修改。</Trans>
                </p>
              </div>
            </div>

            <InfoTile
              icon={ShieldCheck}
              label={<Trans>设备身份</Trans>}
              value={<Trans>仅用于本地展示和配对确认</Trans>}
            />
          </div>

          <div className="flex flex-col justify-between gap-6">
            <div className="glass-control rounded-[22px] p-4">
              <label
                htmlFor="device-name-input"
                className="text-sm font-medium text-foreground"
              >
                <Trans>设备名称</Trans>
              </label>
              <Input
                id="device-name-input"
                value={name}
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !disabled) onConfirm();
                }}
                autoFocus
                maxLength={40}
                placeholder={t`例如：MacBook Pro`}
                className="mt-2 h-12 rounded-[16px] bg-white/55 text-base dark:bg-white/[0.06]"
              />
              {error !== null ? (
                <p className="mt-2 text-xs text-destructive">{error}</p>
              ) : (
                <p className="mt-2 text-xs text-muted-foreground">
                  <Trans>建议使用你能在设备列表中快速识别的名称。</Trans>
                </p>
              )}
            </div>

            <div className="flex justify-end">
              <TaskButton
                onClick={onConfirm}
                disabled={disabled}
                size="lg"
                className="h-11"
              >
                {saving ? (
                  <Trans>正在保存...</Trans>
                ) : (
                  <>
                    <Trans>进入 SwarmDrop</Trans>
                    <ArrowRight className="size-4" />
                  </>
                )}
              </TaskButton>
            </div>
          </div>
        </div>
      </GlassPanel>
    </div>
  );
}
