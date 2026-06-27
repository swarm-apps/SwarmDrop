import { useEffect, useState } from "react";
import { Ban, Shield, ShieldAlert, ShieldCheck } from "lucide-react";
import { Trans, useLingui } from "@lingui/react/macro";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type {
  Device,
  DeviceReceivePolicy,
  DeviceTrustLevel,
} from "@/lib/bindings";
import { deviceDisplayName } from "@/lib/device-name";
import { getErrorMessage } from "@/lib/errors";
import { pickFolder } from "@/lib/file-picker";

interface TrustPolicyDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  device: Device;
  onSubmit: (
    device: Device,
    trustLevel: DeviceTrustLevel,
    receivePolicy: DeviceReceivePolicy,
  ) => Promise<void>;
}

export function TrustPolicyDialog({
  open,
  onOpenChange,
  device,
  onSubmit,
}: TrustPolicyDialogProps) {
  const { t } = useLingui();
  const [trustLevel, setTrustLevel] = useState<DeviceTrustLevel>(
    device.trustLevel ?? "collaborator",
  );
  const [policy, setPolicy] = useState<DeviceReceivePolicy>(() =>
    normalizePolicy(device),
  );
  const [limitMb, setLimitMb] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    const nextPolicy = normalizePolicy(device);
    setTrustLevel(device.trustLevel ?? "collaborator");
    setPolicy(nextPolicy);
    setLimitMb(
      nextPolicy.maxTransferBytes
        ? String(Math.ceil(nextPolicy.maxTransferBytes / 1024 / 1024))
        : "",
    );
  }, [device, open]);

  const updateTrustLevel = (value: DeviceTrustLevel) => {
    const next = defaultPolicyForTrust(value, policy);
    setTrustLevel(value);
    setPolicy(next);
    setLimitMb(
      next.maxTransferBytes
        ? String(Math.ceil(next.maxTransferBytes / 1024 / 1024))
        : "",
    );
  };

  const chooseDefaultSaveLocation = async () => {
    const selected = await pickFolder();
    if (selected) {
      setPolicy((current) => ({
        ...current,
        defaultSaveLocation: selected,
      }));
    }
  };

  const handleSubmit = async () => {
    setSaving(true);
    try {
      const maxTransferBytes = limitMb.trim()
        ? Math.max(0, Number(limitMb) || 0) * 1024 * 1024
        : null;
      await onSubmit(device, trustLevel, {
        ...policy,
        maxTransferBytes,
        saveBehavior: "inbox_and_default_save_location",
      });
      onOpenChange(false);
    } catch (err) {
      toast.error(getErrorMessage(err));
    } finally {
      setSaving(false);
    }
  };

  const autoAcceptDisabled = trustLevel === "blocked";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-[520px]"
        onClick={(event) => event.stopPropagation()}
      >
        <DialogHeader>
          <DialogTitle>
            <Trans>信任策略</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>为「{deviceDisplayName(device)}」设置接收规则</Trans>
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-4 py-1">
          <div className="grid gap-2">
            <Label>
              <Trans>信任级别</Trans>
            </Label>
            <Select
              value={trustLevel}
              onValueChange={(value) =>
                updateTrustLevel(value as DeviceTrustLevel)
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="owned">{t`本人设备`}</SelectItem>
                <SelectItem value="collaborator">{t`协作者`}</SelectItem>
                <SelectItem value="temporary">{t`临时设备`}</SelectItem>
                <SelectItem value="blocked">{t`已阻止`}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <PolicySwitch
            label={t`自动接收`}
            description={t`启用后，符合策略的入站文件会直接进入收件箱`}
            checked={policy.autoAccept}
            disabled={autoAcceptDisabled}
            onCheckedChange={(checked) =>
              setPolicy((current) => ({
                ...current,
                autoAccept: checked,
                requireConfirmation: !checked,
              }))
            }
          />

          <PolicySwitch
            label={t`允许文件夹`}
            description={t`关闭后，包含子路径的传输会被策略拒绝`}
            checked={policy.allowDirectories}
            disabled={trustLevel === "blocked"}
            onCheckedChange={(checked) =>
              setPolicy((current) => ({
                ...current,
                allowDirectories: checked,
              }))
            }
          />

          <PolicySwitch
            label={t`允许中继自动接收`}
            description={t`关闭后，通过中继连接的传输仍需手动确认`}
            checked={policy.allowRelayAutoAccept}
            disabled={!policy.autoAccept || trustLevel === "blocked"}
            onCheckedChange={(checked) =>
              setPolicy((current) => ({
                ...current,
                allowRelayAutoAccept: checked,
              }))
            }
          />

          <div className="grid gap-2">
            <Label htmlFor="trust-policy-limit">
              <Trans>大小上限</Trans>
            </Label>
            <div className="flex items-center gap-2">
              <Input
                id="trust-policy-limit"
                inputMode="numeric"
                value={limitMb}
                placeholder={t`不限制`}
                disabled={trustLevel === "blocked"}
                onChange={(event) => setLimitMb(event.target.value)}
              />
              <span className="shrink-0 text-xs text-muted-foreground">MB</span>
            </div>
          </div>

          <div className="grid gap-2">
            <Label>
              <Trans>自动接收位置</Trans>
            </Label>
            <div className="flex min-w-0 items-center gap-2 rounded-lg border border-border px-3 py-2">
              <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground">
                {policy.defaultSaveLocation || t`未设置`}
              </span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 px-2 text-xs"
                onClick={chooseDefaultSaveLocation}
                disabled={trustLevel === "blocked"}
              >
                <Trans>选择</Trans>
              </Button>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            <Trans>取消</Trans>
          </Button>
          <Button onClick={handleSubmit} disabled={saving}>
            {saving ? <Trans>保存中...</Trans> : <Trans>保存策略</Trans>}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function PolicySwitch({
  label,
  description,
  checked,
  disabled,
  onCheckedChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  disabled?: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-border px-3 py-2.5">
      <div className="grid gap-0.5">
        <span className="text-sm font-medium text-foreground">{label}</span>
        <span className="text-xs text-muted-foreground">{description}</span>
      </div>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={onCheckedChange}
      />
    </div>
  );
}

function normalizePolicy(device: Device): DeviceReceivePolicy {
  return {
    ...defaultPolicyForTrust(device.trustLevel ?? "collaborator"),
    ...device.receivePolicy,
    saveBehavior: "inbox_and_default_save_location",
  };
}

function defaultPolicyForTrust(
  trustLevel: DeviceTrustLevel,
  previous?: DeviceReceivePolicy,
): DeviceReceivePolicy {
  const defaultSaveLocation = previous?.defaultSaveLocation ?? null;
  if (trustLevel === "owned") {
    return {
      autoAccept: true,
      requireConfirmation: false,
      maxTransferBytes: null,
      allowDirectories: true,
      allowRelayAutoAccept: true,
      saveBehavior: "inbox_and_default_save_location",
      defaultSaveLocation,
      allowMcpSendToDevice: true,
      expiresAt: null,
    };
  }
  if (trustLevel === "temporary") {
    return {
      autoAccept: false,
      requireConfirmation: true,
      maxTransferBytes: 512 * 1024 * 1024,
      allowDirectories: false,
      allowRelayAutoAccept: false,
      saveBehavior: "inbox_and_default_save_location",
      defaultSaveLocation,
      allowMcpSendToDevice: false,
      expiresAt: Date.now() + 24 * 60 * 60 * 1000,
    };
  }
  if (trustLevel === "blocked") {
    return {
      autoAccept: false,
      requireConfirmation: false,
      maxTransferBytes: 0,
      allowDirectories: false,
      allowRelayAutoAccept: false,
      saveBehavior: "inbox_and_default_save_location",
      defaultSaveLocation: null,
      allowMcpSendToDevice: false,
      expiresAt: null,
    };
  }
  return {
    autoAccept: false,
    requireConfirmation: true,
    maxTransferBytes: null,
    allowDirectories: true,
    allowRelayAutoAccept: false,
    saveBehavior: "inbox_and_default_save_location",
    defaultSaveLocation,
    allowMcpSendToDevice: false,
    expiresAt: null,
  };
}

export function trustConfig(trustLevel: DeviceTrustLevel) {
  switch (trustLevel) {
    case "owned":
      return {
        icon: ShieldCheck,
        label: <Trans>本人设备</Trans>,
        className:
          "bg-emerald-50 text-emerald-700 ring-emerald-600/10 dark:bg-emerald-500/12 dark:text-emerald-300 dark:ring-emerald-400/15",
      };
    case "temporary":
      return {
        icon: ShieldAlert,
        label: <Trans>临时设备</Trans>,
        className:
          "bg-amber-50 text-amber-700 ring-amber-600/10 dark:bg-amber-500/12 dark:text-amber-300 dark:ring-amber-400/15",
      };
    case "blocked":
      return {
        icon: Ban,
        label: <Trans>已阻止</Trans>,
        className:
          "bg-red-50 text-red-700 ring-red-600/10 dark:bg-red-500/12 dark:text-red-300 dark:ring-red-400/15",
      };
    case "collaborator":
    default:
      return {
        icon: Shield,
        label: <Trans>协作者</Trans>,
        className:
          "bg-blue-50 text-blue-700 ring-blue-600/10 dark:bg-blue-500/12 dark:text-blue-300 dark:ring-blue-400/15",
      };
  }
}
