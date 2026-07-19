/**
 * 邀请二维码组件
 *
 * SVG 由后端 `invite_qr_svg` 生成（三端统一编码规范：大写 alphanumeric + ECL::M +
 * quiet zone，见 `crates/invite/src/qr.rs`）——前端只负责渲染进白卡。
 *
 * 二维码本体固定深模块 + 白底、不随暗色主题反色（摄像头对反色 QR 识别差）。
 */

import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { commands } from "@/lib/bindings";

interface InviteQrProps {
  /** 邀请串（小写规范形态）；null 时展示占位 */
  invite: string | null;
  /** 边长（px），默认 260 */
  size?: number;
  className?: string;
}

export function InviteQr({ invite, size = 260, className }: InviteQrProps) {
  const [svg, setSvg] = useState<string | null>(null);

  useEffect(() => {
    if (invite === null) {
      setSvg(null);
      return;
    }
    let cancelled = false;
    commands
      .inviteQrSvg(invite)
      .then((s) => {
        if (!cancelled) setSvg(s);
      })
      .catch(() => {
        if (!cancelled) setSvg(null);
      });
    return () => {
      cancelled = true;
    };
  }, [invite]);

  return (
    <div
      className={`flex items-center justify-center rounded-2xl bg-white p-3 shadow-sm ${className ?? ""}`}
      style={{ width: size + 24, height: size + 24 }}
    >
      {svg ? (
        <div
          style={{ width: size, height: size }}
          // 二维码 SVG 由后端受信任生成（纯几何 path，无脚本），安全内联
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      ) : (
        <Loader2 className="size-8 animate-spin text-neutral-400" />
      )}
    </div>
  );
}
