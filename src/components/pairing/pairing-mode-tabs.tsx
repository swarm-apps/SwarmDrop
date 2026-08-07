/**
 * 配对模式切换——「展示邀请 / 粘贴邀请」。
 *
 * 两个模式各自是一条路由（`/pairing/generate`、`/pairing/input`），但对用户是同一件事
 * 的两个方向，所以挂在 TaskToolbar 的 trailing 位上，读起来像同一屏内的分段切换，
 * 不必退回设备页再选另一个入口。
 */

import { useNavigate, useRouterState } from "@tanstack/react-router";
import { ClipboardPaste, QrCode } from "lucide-react";
import { t } from "@lingui/core/macro";
import { Trans } from "@lingui/react/macro";
import { SegmentedControl } from "@/components/layout/section-primitives";

const MODES = [
  {
    value: "/pairing/generate",
    icon: QrCode,
    label: <Trans>展示邀请</Trans>,
    testid: "pairing-mode-generate",
  },
  {
    value: "/pairing/input",
    icon: ClipboardPaste,
    label: <Trans>粘贴邀请</Trans>,
    testid: "pairing-mode-input",
  },
] as const;

type PairingMode = (typeof MODES)[number]["value"];

export function PairingModeTabs() {
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (s) => s.location.pathname });

  return (
    <SegmentedControl<PairingMode>
      variant="tabs"
      size="md"
      label={t`配对方式`}
      // 精确匹配：前缀匹配会把未来的 /pairing/generate/xxx 也算成当前模式
      value={MODES.find((m) => m.value === pathname)?.value ?? "/pairing/generate"}
      options={MODES.map((m) => ({ ...m }))}
      onChange={(to) => navigate({ to })}
    />
  );
}
