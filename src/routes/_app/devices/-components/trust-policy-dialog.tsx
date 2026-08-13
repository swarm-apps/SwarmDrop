import { useEffect, useRef, useState } from "react";
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
import { commands } from "@/lib/bindings";
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
  const [policy, setPolicy] = useState<DeviceReceivePolicy | null>(
    device.receivePolicy,
  );
  const [limitMb, setLimitMb] = useState("");
  const [saving, setSaving] = useState(false);
  /** 派生的轮次，用来丢弃过期结果（见 `updateTrustLevel`）。 */
  const deriveSeq = useRef(0);

  useEffect(() => {
    if (!open) return;
    setTrustLevel(device.trustLevel ?? "collaborator");
    setPolicy(device.receivePolicy);
    setLimitMb(
      device.receivePolicy?.maxTransferBytes
        ? String(Math.ceil(device.receivePolicy.maxTransferBytes / 1024 / 1024))
        : "",
    );
  }, [device, open]);

  /** 局部改一项策略。`policy` 为 null 时是 no-op——那时开关根本没渲染（见下方守卫）。 */
  const patchPolicy = (patch: Partial<DeviceReceivePolicy>) =>
    setPolicy((current) => (current ? { ...current, ...patch } : current));

  // 切级别时的默认策略**向内核要**，不在前端算。
  //
  // 那张表此前在这里抄了一份、移动端抄了另一份，两份还长出了不同的「保留哪些字段」规则，
  // 而内核那一份一个都不保留——同一个产品动作三种行为。现在规则只在
  // `DeviceReceivePolicy::for_trust_level` 一处，这里只是把用户当前的策略递过去。
  //
  // 它是个纯派生命令（不取 State），失败只可能是 IPC 本身出问题——那时保持原策略不动
  // 比把界面重置成一个猜出来的值诚实。
  //
  // **级别与策略一起提交**：派生成功之前不动 `trustLevel`。否则失败时会留下「级别已变、
  // 策略还是旧的」这一对，而用户可以就这么点保存——`update_policy` 对传入的策略不做钳制，
  // 于是存下一台「已阻止但开关还写着自动接收」的设备。（拦得住：`evaluate_receive_policy`
  // 先判 `trust_level == Blocked` 再看策略，所以那是显示不一致而不是安全缺口。）
  //
  // 序号丢弃过期结果：连点两个级别时，先发的那次可能后 resolve，把 A 的默认值盖到 B 上。
  const updateTrustLevel = async (value: DeviceTrustLevel) => {
    const seq = ++deriveSeq.current;
    try {
      const next = await commands.defaultReceivePolicy(value, policy);
      if (seq !== deriveSeq.current) return;
      setTrustLevel(value);
      setPolicy(next);
      setLimitMb(
        next.maxTransferBytes
          ? String(Math.ceil(next.maxTransferBytes / 1024 / 1024))
          : "",
      );
    } catch (err) {
      if (seq !== deriveSeq.current) return;
      toast.error(getErrorMessage(err));
    }
  };

  const chooseDefaultSaveLocation = async () => {
    const selected = await pickFolder();
    if (selected) {
      patchPolicy({ defaultSaveLocation: selected });
    }
  };

  // 解析大小上限：非空但非正数/非法 → 视为非法输入，不静默兜底成 0（0 在 core 表示
  // “拒收一切”）。仅合法非空才换算字节，空或非法都按 null（不限制）。
  const trimmedLimit = limitMb.trim();
  const parsedLimit = Number(trimmedLimit);
  const limitInvalid =
    trimmedLimit !== "" && (!Number.isFinite(parsedLimit) || parsedLimit <= 0);
  const maxTransferBytes =
    trimmedLimit !== "" && Number.isFinite(parsedLimit) && parsedLimit > 0
      ? Math.floor(parsedLimit) * 1024 * 1024
      : null;

  const handleSubmit = async () => {
    if (limitInvalid || !policy) return;
    setSaving(true);
    try {
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

          {/* 已配对设备恒带策略（`PairedDeviceInfo::new` 就给了一份）。null 只可能来自
              尚未配对的条目——本对话框不为它们打开，这层守卫是为了不再需要一份
              「策略缺失时用什么」的前端默认表。 */}
          {policy && (
            <>
            <PolicySwitch
              label={t`自动接收`}
              description={t`启用后，符合策略的入站文件会直接进入收件箱`}
              checked={policy.autoAccept}
              disabled={autoAcceptDisabled}
              onCheckedChange={(checked) =>
                patchPolicy({
                  autoAccept: checked,
                  requireConfirmation: !checked,
                })
              }
            />

            <PolicySwitch
              label={t`允许文件夹`}
              description={t`关闭后，包含子路径的传输会被策略拒绝`}
              checked={policy.allowDirectories}
              disabled={trustLevel === "blocked"}
              onCheckedChange={(checked) =>
                patchPolicy({ allowDirectories: checked })
              }
            />

            <PolicySwitch
              label={t`允许中继自动接收`}
              description={t`关闭后，通过中继连接的传输仍需手动确认`}
              checked={policy.allowRelayAutoAccept}
              disabled={!policy.autoAccept || trustLevel === "blocked"}
              onCheckedChange={(checked) =>
                patchPolicy({ allowRelayAutoAccept: checked })
              }
            />

            <PolicySwitch
              label={t`允许 MCP/AI 代收`}
              description={t`允许本机 AI 助手代为处置该设备需你确认的入站文件（接受或拒绝）；关闭则仍需你手动确认。已自动接收的入站不受此影响`}
              checked={policy.allowMcpAcceptFromDevice ?? false}
              disabled={trustLevel === "blocked"}
              onCheckedChange={(checked) =>
                patchPolicy({ allowMcpAcceptFromDevice: checked })
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
                  aria-invalid={limitInvalid}
                  disabled={trustLevel === "blocked"}
                  onChange={(event) => setLimitMb(event.target.value)}
                />
                <span className="shrink-0 text-xs text-muted-foreground">MB</span>
              </div>
              {limitInvalid ? (
                <span className="text-xs text-destructive">
                  <Trans>请输入大于 0 的数字，留空表示不限制</Trans>
                </span>
              ) : null}
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
            </>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            <Trans>取消</Trans>
          </Button>
          <Button onClick={handleSubmit} disabled={saving || limitInvalid}>
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

/**
 * 信任级别的呈现。**四级的色彩语义三端同一套**（Web `TRUST_META`、移动 `trust-badge.tsx`）：
 *
 *   owned        品牌色 —— 唯一的饱和色留给最高信任（One Accent Rule）
 *   collaborator 中性 —— 它是默认级别，多数设备都是它；上色会让整屏徽标全是彩的
 *   temporary    warning
 *   blocked      destructive
 *
 * 三端此前在前两级上各跑各的：桌面 owned 是绿、collaborator 反倒是品牌青绿，
 * 移动把 collaborator 涂成 success 绿（凭空多出第二个饱和色）。
 */
export function trustConfig(trustLevel: DeviceTrustLevel) {
  switch (trustLevel) {
    case "owned":
      return {
        icon: ShieldCheck,
        label: <Trans>本人设备</Trans>,
        className: "bg-primary/12 text-brand ring-primary/15",
      };
    case "temporary":
      return {
        icon: ShieldAlert,
        label: <Trans>临时设备</Trans>,
        className: "bg-warning/15 text-warning-ink ring-warning/20",
      };
    case "blocked":
      return {
        icon: Ban,
        label: <Trans>已阻止</Trans>,
        className: "bg-destructive/12 text-destructive-ink ring-destructive/15",
      };
    case "collaborator":
    default:
      return {
        icon: Shield,
        label: <Trans>协作者</Trans>,
        className: "bg-muted text-muted-foreground ring-border",
      };
  }
}
