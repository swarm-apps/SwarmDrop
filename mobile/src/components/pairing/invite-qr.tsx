/**
 * 邀请二维码（移动端）
 *
 * 模块矩阵由 core `inviteQrMatrix` 生成（三端统一编码规范：原样编码 + 最优分段 +
 * ECL::M + quiet zone，见 crates/invite/src/qr.rs），本组件用 react-native-svg 把深模块
 * 合并成单条 `<Path>`。深模块 + 白底、不随暗色主题反色（摄像头对反色 QR 识别差）。
 *
 * 白卡内一切文字都固定 slate 深色，**不能**用 `text-muted-foreground` 这类主题 token
 * ——暗色主题下会变浅灰，压在白底上不可读。
 *
 * 码位永远占同一块白卡：等待 / 过期 / 出错以覆盖层压在码面上，不替换整块内容，
 * 状态一眼可见且布局不跳（PRODUCT.md「状态诚实可见」）。
 */

import { useLingui } from "@lingui/react/macro";
import { AlertCircle, Hourglass, QrCode, TimerOff } from "lucide-react-native";
import { memo, useMemo } from "react";
import { View } from "react-native";
import Svg, { Path } from "react-native-svg";
import { Text } from "@/components/ui/text";
import { getMobileCore } from "@/core/mobile-core";
import { cn } from "@/lib/utils";

/** 码面覆盖态：邀请本身无效（过期 / 出错 / 还没生成），码留在原位但明确失效。 */
export interface InviteQrOverlay {
  kind: "waiting" | "blocked" | "expired" | "error";
  message: string;
}

interface InviteQrProps {
  /** canonical 邀请链接（原样传给后端编码，不做大小写归一）；null 时展示骨架 */
  invite: string | null;
  /** 边长（px），默认 220 */
  size?: number;
  /** 覆盖态；null 表示码有效可扫 */
  overlay?: InviteQrOverlay | null;
}

/**
 * 整张码压成**一条** `<Path>`：先按行做 run-length 合并连续深模块，再把每段拼成
 * `M x y h w v1 h-w z` 子路径。裸格子画法是 ~2600 个 `<Rect>`，合并后约 900 段，
 * 最终只落 1 个原生节点——父组件重渲时 Fabric 侧不用 diff 上千个子节点。
 */
interface QrGeometry {
  dim: number;
  path: string;
}

/** 图标按「谁在动」区分：waiting 有人在处理，blocked 等你动手。 */
const OVERLAY_ICON = {
  waiting: Hourglass,
  blocked: QrCode,
  expired: TimerOff,
  error: AlertCircle,
} as const;

const MODULE_COLOR = "#0a0a0a";

export const InviteQr = memo(function InviteQr({
  invite,
  // ⚠️ 这是**白卡外框**，二维码本体是它减掉两侧 12 的内边距 = 196。传给 Rust 的必须是
  // 后者（`inner`），它同时是地址提示的预算：放不下时后端按价值反序回收地址（circuit
  // 永不丢）。传成 `size` 等于虚报一圈码面，真机上就是扫不出来。
  size = 220,
  overlay = null,
}: InviteQrProps) {
  const { t } = useLingui();

  // 白卡容器（padding 12），二维码本体铺满
  const pad = 12;
  // 夹到 0：`inner` 要过 uniffi 的 u32，负数会在转换器里抛一个没有上下文的错。
  // 夹住之后走 Rust 的码面下限判断（`qr::MIN_FACE_PX`），错误信息里会写清楚是码面太小。
  const inner = Math.max(0, size - pad * 2);

  // inviteQrMatrix 是同步 uniffi 方法（Rust pub fn）→ useMemo 直接算，无首帧 spinner 闪烁
  const geometry = useMemo<QrGeometry | null>(() => {
    if (invite === null) return null;
    try {
      const m = getMobileCore().inviteQrMatrix(invite, inner);
      const dim = Number(m.size);
      const modules = m.modules;
      let path = "";
      for (let y = 0; y < dim; y++) {
        let runStart = -1;
        for (let x = 0; x <= dim; x++) {
          const on = x < dim && modules[y * dim + x] === true;
          if (on && runStart < 0) runStart = x;
          else if (!on && runStart >= 0) {
            const w = x - runStart;
            path += `M${runStart} ${y}h${w}v1h-${w}z`;
            runStart = -1;
          }
        }
      }
      return { dim, path };
    } catch {
      return null;
    }
    // inner 进依赖不是形式主义：它是后端的地址预算，改了尺寸要重新裁一次
  }, [invite, inner]);

  // 矩阵算不出来自成一态：折进覆盖层，与过期/等待共用一套呈现
  const effectiveOverlay: InviteQrOverlay | null =
    invite !== null && geometry === null
      ? { kind: "error", message: t`二维码生成失败` }
      : overlay;

  return (
    <View
      className="items-center justify-center rounded-lg bg-white"
      style={{ width: size, height: size, padding: pad }}
      accessibilityRole="image"
      accessibilityLabel={t`配对邀请二维码`}
    >
      {geometry ? (
        <Svg
          width={inner}
          height={inner}
          viewBox={`0 0 ${geometry.dim} ${geometry.dim}`}
          // 压到扫不动为止：失效的码若还能被读到，对方只会白跑一趟失败流程
          opacity={effectiveOverlay ? 0.09 : 1}
        >
          {/* 白底已由容器提供，只画深模块 */}
          <Path d={geometry.path} fill={MODULE_COLOR} />
        </Svg>
      ) : (
        <QrSkeleton size={inner} />
      )}

      {effectiveOverlay ? <QrOverlay {...effectiveOverlay} /> : null}
    </View>
  );
});

/** 骨架：三枚定位角轮廓，示意「这里将出现一个二维码」——比居中转圈更贴合内容形状。 */
function QrSkeleton({ size }: { size: number }) {
  const corner = Math.round(size * 0.22);
  const style = {
    position: "absolute",
    width: corner,
    height: corner,
    borderWidth: 5,
    borderColor: "rgba(15,23,42,0.09)",
    borderRadius: 6,
  } as const;
  return (
    <View style={{ width: size, height: size }}>
      <View style={[style, { left: 0, top: 0 }]} />
      <View style={[style, { right: 0, top: 0 }]} />
      <View style={[style, { left: 0, bottom: 0 }]} />
    </View>
  );
}

function QrOverlay({ kind, message }: InviteQrOverlay) {
  const Icon = OVERLAY_ICON[kind];
  const isError = kind === "error";
  return (
    <View className="absolute inset-0 items-center justify-center gap-2.5 px-6">
      <View
        className={cn(
          "size-11 items-center justify-center rounded-full",
          isError ? "bg-red-50" : "bg-slate-100",
        )}
      >
        <Icon color={isError ? "#dc2626" : "#64748b"} size={20} />
      </View>
      {/* 白底固定，文字色不接主题 token */}
      <Text className="text-center text-[13px] font-medium text-slate-700">
        {message}
      </Text>
    </View>
  );
}
