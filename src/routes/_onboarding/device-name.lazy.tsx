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
import { ArrowRight, MonitorSmartphone } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { hostname as readHostname } from "@tauri-apps/plugin-os";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { applyDeviceName } from "@/lib/device-name";
import { getErrorMessage } from "@/lib/errors";
import { usePreferencesStore } from "@/stores/preferences-store";

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
    <div className="mx-auto flex h-full w-full max-w-md flex-col gap-8 px-8 py-12">
      <div className="flex flex-col items-center gap-4">
        <div className="flex size-20 items-center justify-center rounded-full bg-blue-50 dark:bg-blue-900/20">
          <MonitorSmartphone className="size-9 text-blue-600 dark:text-blue-400" />
        </div>
        <div className="flex flex-col items-center gap-2 text-center">
          <h1 className="text-2xl font-bold text-foreground">
            <Trans>给设备取个名字</Trans>
          </h1>
          <p className="text-sm leading-6 text-muted-foreground">
            <Trans>
              其他设备配对时会看到这个名称
              <br />
              你可以随时在「设置」里修改
            </Trans>
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-2">
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
          className="h-11 rounded-lg text-base"
        />
        {error !== null ? (
          <p className="text-xs text-destructive">{error}</p>
        ) : null}
      </div>

      <div className="mt-auto flex flex-col gap-3">
        <Button
          onClick={onConfirm}
          disabled={disabled}
          size="lg"
          className="h-11 rounded-lg gap-2"
        >
          {saving ? (
            <Trans>正在保存...</Trans>
          ) : (
            <>
              <Trans>进入 SwarmDrop</Trans>
              <ArrowRight className="size-4" />
            </>
          )}
        </Button>
      </div>
    </div>
  );
}
